extern crate ffmpeg_next as ffmpeg;

use std::os::unix::prelude::OsStrExt;

mod ffmpeg_backend;
mod input;
mod interface;
mod state;
mod util;

use clap::Parser;
use ffmpeg::ChannelLayout;
use once_cell::sync::Lazy;
use question::Answer;
use tracing::*;
use tracing_subscriber::EnvFilter;

use crate::input::Stream;

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

    tracing_subscriber::fmt()
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

    let path = ARGS.path.canonicalize()?;

    let entries = if path.is_file() {
        vec![ path.clone()]
    } else if  path.is_dir() {
        let mut v: Vec<_> = std::fs::read_dir(&path)?
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
                    // Special case for `.nfo` and `.txt`: these are never video files.
                    Some("nfo" | "txt") => false,
                    // If there is no extension; assume it is not a video file - ffmpeg would get
                    // confused anyway.
                    None => false,
                    Some(file_extension) => INPUT_FILE_EXTENSIONS
                        .iter()
                        .any(|ext| ext.as_str() == file_extension),
                }
            })
            .collect();
        v.sort_unstable();
        v
    } else {
        error!("Provided path must be either a file or a directory");
        std::process::exit(1);
    };

    debug!(?entries);

    // prepare directory
    let output_dir = if let Some(ref tv_options) = tv_options {
        path.join(format!("Season {:02}", tv_options.season))
    } else {
        path.join("newfiles")
    };
    if output_dir.is_dir() {
        info!(dir = ?output_dir, "Directory already exists");
    } else if ARGS.simulate {
        info!(dir = ?output_dir, "Simulate mode: not creating directory");
    } else {
        std::fs::create_dir(&output_dir)?;
        info!(dir = ?output_dir, "Created directory");
    }

    let mut commands = Vec::with_capacity(entries.len());

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

        let file = ffmpeg::format::input(&input_filepath)?;

        let parsed = input::parse_stream_metadata(file);
        let stream_mappings = input::get_stream_mappings(&parsed);
        let codec_mappings = input::get_codec_mapping(&stream_mappings);

        let mappings = &stream_mappings;
        let codecs = &codec_mappings;
        println!(
            "Input file '{}' -> '{}':",
            input_filepath
                .to_str()
                .expect("Path contained invalid unicode."),
            output_path
                .to_str()
                .expect("Path contained invalid unicode.")
        );
        for stream in mappings.iter() {
            let index = stream.index();
            let codec = codecs.get(&index).unwrap();
            let oldcodec = stream.codec();
            let newcodec = match codec {
                None => &oldcodec,
                Some(x) => x,
            };

            print!("Mapping stream {index}: {oldcodec:?} ");

            if let Stream::Audio(audio) = stream {
                if audio.channel_layout == ChannelLayout::STEREO {
                    print!("(2.0) ");
                } else if audio.channel_layout == ChannelLayout::_5POINT1 {
                    print!("(5.1) ");
                } else if audio.channel_layout == ChannelLayout::_7POINT1 {
                    print!("(7.1) ");
                }
            }

            print!("-> {newcodec:?} ");

            if codec.is_none() {
                print!("(copy) ")
            }

            if matches!(stream, input::Stream::Video(_)) && codec.is_some() {
                let crop = ARGS.crop.is_some();
                // FIXME: fails to specify deinterlacing in log message if the deinterlacing is
                // inferred from the video stream.
                let deinterlace = ARGS.force_deinterlace;
                if crop || deinterlace {
                    print!("(");
                    if crop {
                        print!("crop")
                    }
                    if crop && deinterlace {
                        print!(", deinterlace")
                    } else if deinterlace {
                        print!("deinterlace")
                    }
                    print!(")")
                }
            }
            println!();
        }
        println!();

        let command = ffmpeg_backend::generate_ffmpeg_command(
            input_filepath,
            output_path,
            stream_mappings,
            codec_mappings,
        );

        info!(?command);
        commands.push(command);
    }

    if ARGS.simulate {
        eprintln!("Simulate mode; not executing commands");
        return Ok(());
    }

    if !util::confirm("Continue?", Some(Answer::YES)) {
        eprintln!("Aborting");
        return Ok(());
    }

    match ARGS.parallel {
        Some(jobs) => {
            let jobs = jobs.unwrap_or(usize::MAX);
            let mut running = Vec::new();
            loop {
                while running.len() < jobs && !commands.is_empty() {
                    let mut command = commands.pop().unwrap();
                    let child = command.spawn()?;
                    running.push(child);
                }

                if running.is_empty() {
                    break;
                }

                for i in 0..running.len() {
                    let child = &mut running[i];
                    match child.try_wait()? {
                        Some(status) => {
                            if !status.success() {
                                for proc in &mut running {
                                    proc.kill()?;
                                }
                                eprintln!("Command failed");
                                return Ok(());
                            }
                            running.remove(i);
                        }
                        None => {}
                    }
                }
            }
        }
        None => {
            for mut command in commands {
                command.spawn()?.wait()?;
            }
        }
    }

    Ok(())
}
