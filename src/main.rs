extern crate ffmpeg_next as ffmpeg;

use std::collections::HashMap;
use std::os::unix::prelude::OsStrExt;

mod backend;
mod frontend;
mod interface;
mod state;
mod util;

use ffmpeg::codec;
use frontend::StreamMappings;
use once_cell::sync::Lazy;
use clap::Parser;
use tracing::*;

static ARGS: Lazy<interface::Args> = Lazy::new(interface::Args::parse);

const EXEMPT_FILE_EXTENSIONS: [&str; 11] = [
    "clbin", "gif", "jpg", "md", "nfo", "png", "py", "rar", "sfv", "srr", "txt",
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    ffmpeg::init()?;

    if std::env::var("RUST_LOG").is_err() {
        std::env::set_var("RUST_LOG", "videoconverter=info");
    }

    tracing_subscriber::fmt().pretty().init();

    validate_args();

    debug!(?ARGS);

    // Shut libav* up
    // Safety: Calling c function, modifying global state
    unsafe {
        ffmpeg::ffi::av_log_set_level(ffmpeg::ffi::AV_LOG_FATAL);
    }

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

    let entries = {
        let mut v: Vec<_> = std::fs::read_dir(&ARGS.path)?
            .map(|entry| entry.unwrap().path())
            .filter(|path| !path.is_dir()) // Remove directories
            .filter(|path| {
                // Remove files that start with '.'
                !path
                    .file_name()
                    .map(|filename| filename.as_bytes().starts_with(b"."))
                    .unwrap_or(true) // Remove files that have no filename (?)
            })
            .filter(|path| {
                // Remove files with extensions that are exempt
                let file_extension = match path.extension().and_then(|x| x.to_str()) {
                    Some(x) => x,
                    None => {
                        // Remove filles with no extension
                        return false;
                    }
                };
                // Remove files of the form `*.r00`, `*.r01`, etc
                let is_rar_segment = file_extension.starts_with('r')
                    && file_extension[1..].chars().all(|c| c.is_digit(10));

                !(EXEMPT_FILE_EXTENSIONS.contains(&file_extension) || is_rar_segment)
            })
            .collect();
        v.sort_unstable();
        v
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

    for input_path in entries {
        let output_filename = backend::generate_output_filename(&input_path, &tv_options);
        let output_path = if let Some(ref tv_options) = tv_options {
            input_path
                .parent()
                .expect("Somehow the input_path was root")
                .join(format!("Season {:02}", tv_options.season))
        } else {
            input_path
                .parent()
                .expect("Somehow the input_path was root")
                .join("newfiles")
        }
        .join(output_filename);

        if let Some(ref mut tv_options) = tv_options {
            tv_options.episode += 1;
        }

        info!(
            "Mapping '{}' --> '{}'",
            input_path
                .to_str()
                .expect("Path contained invalid unicode."),
            output_path
                .to_str()
                .expect("Path contained invalid unicode.")
        );

        let file = ffmpeg::format::input(&input_path)?;

        let parsed = frontend::parse_stream_metadata(file);
        let stream_mappings = frontend::get_stream_mappings(parsed);
        let codec_mappings = frontend::get_codec_mapping(&stream_mappings);

        log_mappings(&stream_mappings, &codec_mappings);

        let mut command = backend::generate_ffmpeg_command(
            input_path,
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

fn validate_args() {
    if matches!(ARGS.encoder, interface::VideoEncoder::Nvenc) {
        if ARGS.no_hwaccel {
            eprintln!("Hardware acceleration cannot be disabled when using nvenc");
            std::process::exit(1);
        }
        if ARGS.tune.is_some() {
            eprintln!("Libx264 tunes cannot be used with nvenc.");
            std::process::exit(1);
        }
    }
    if ARGS.force_deinterlace && ARGS.no_deinterlace {
        eprintln!("The arguments `--force-deinterlace` and `--no-deinterlace` are incompatible.");
        std::process::exit(1);
    }
}
