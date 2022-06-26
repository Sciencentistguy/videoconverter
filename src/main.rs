extern crate ffmpeg_next as ffmpeg;

use std::collections::HashMap;
use std::os::unix::prelude::OsStrExt;

mod ffmpeg_backend;
mod input;
mod interface;
mod state;
mod util;

use clap::Parser;
use ffmpeg::codec;
use input::StreamMappings;
use once_cell::sync::Lazy;
use tracing::*;
use tracing_subscriber::EnvFilter;

static ARGS: Lazy<interface::Args> = Lazy::new(interface::Args::parse);

static INPUT_FILE_EXTENSIONS: Lazy<Vec<String>> = Lazy::new(|| {
    use ffmpeg::format::format::Format;
    ffmpeg_next::format::format::list()
        .filter(|x| matches!(x, Format::Input(_)))
        .flat_map(|x| {
            x.extensions()
                .into_iter()
                .map(|y| y.to_owned())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>()
});

fn main() -> Result<(), Box<dyn std::error::Error>> {
    ffmpeg::init()?;

    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "videoconverter=info");
    }

    tracing_subscriber::fmt()
        .pretty()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    interface::validate_args(&ARGS);

    debug!(?ARGS);

    // Shut libav* up
    unsafe { ffmpeg::ffi::av_log_set_level(ffmpeg::ffi::AV_LOG_FATAL) };

    let mut tv_options = interface::get_tv_options();

    if let Some(ref tv_options) = tv_options {
        if let Err(e) = state::write_state(tv_options) {
            warn!(
                error = %e,
                "Failed to write statefile /tmp/videoconverter.state"
            );
        }
    }

    debug!(?tv_options);

    let entries = if ARGS.path.is_file() {
        vec![ARGS.path.clone()]
    } else if ARGS.path.is_dir() {
        let mut v: Vec<_> = std::fs::read_dir(&ARGS.path)?
            .map(|entry| entry.unwrap().path())
            .filter(|path| !path.is_dir()) // Remove directories
            .filter(|path| {
                // Remove files that start with '.'
                path.file_name()
                    .map(|filename| !filename.as_bytes().starts_with(b"."))
                    .unwrap_or(false) // Remove files that have no filename (?)
            })
            .filter(|path| {
                // Only consider files that ffmpeg can actually take as input
                match path.extension().and_then(|x| x.to_str()) {
                    Some("nfo") => false,
                    Some(file_extension) => INPUT_FILE_EXTENSIONS
                        .iter()
                        .any(|ext| ext.as_str() == file_extension),
                    None => false,
                }
            })
            .collect();
        v.sort_unstable();
        v
    } else {
        unreachable!("`path` must be a file or directory");
    };

    debug!(?entries);

    // prepare directory
    let output_dir = if let Some(ref tv_options) = tv_options {
        ARGS.path.join(format!("Season {:02}", tv_options.season))
    } else {
        ARGS.path.join("newfiles")
    };
    if output_dir.is_dir() {
        info!(dir = ?output_dir, "Directory already exists");
    } else if ARGS.simulate {
        info!(dir = ?output_dir, "Simulate mode: not creating directory");
    } else {
        std::fs::create_dir(&output_dir)?;
        info!(dir = ?output_dir, "Created directory");
    }

    for input_filepath in entries {
        let output_filename =
            ffmpeg_backend::generate_output_filename(&input_filepath, &tv_options);
        let output_path = if let Some(ref tv_options) = tv_options {
            input_filepath
                .parent()
                .expect("input_filepath should have a parent")
                .join(format!("Season {:02}", tv_options.season))
        } else {
            input_filepath
                .parent()
                .expect("input_filepath should have a parent")
                .join("newfiles")
        }
        .join(output_filename);

        if let Some(ref mut tv_options) = tv_options {
            tv_options.episode += 1;
        }

        info!(
            "Mapping '{}' --> '{}'",
            input_filepath
                .to_str()
                .expect("Path contained invalid unicode."),
            output_path
                .to_str()
                .expect("Path contained invalid unicode.")
        );

        let file = ffmpeg::format::input(&input_filepath)?;

        let parsed = input::parse_stream_metadata(file);
        let stream_mappings = input::get_stream_mappings(parsed);
        let codec_mappings = input::get_codec_mapping(&stream_mappings);

        log_mappings(&stream_mappings, &codec_mappings);

        let mut command = ffmpeg_backend::generate_ffmpeg_command(
            input_filepath,
            output_path,
            stream_mappings,
            codec_mappings,
        );

        info!(?command);

        if !ARGS.simulate {
            if ARGS.parallel {
                command.spawn()?;
            } else {
                command.status()?;
            }
        }
    }

    Ok(())
}

#[inline]
fn log_mappings(mappings: &StreamMappings, codecs: &HashMap<usize, Option<codec::Id>>) {
    for stream in mappings.iter() {
        let index = stream.index();
        let codec = codecs.get(&index).unwrap();
        let oldcodec = stream.codec();
        let newcodec = match codec {
            None => &oldcodec,
            Some(x) => x,
        };

        info!(
            "Mapping stream {}: {:?} -> {:?}{}",
            index,
            oldcodec,
            newcodec,
            if codec.is_none() { " (copy)" } else { "" }
        );
    }
}
