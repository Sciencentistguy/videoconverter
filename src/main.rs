extern crate ffmpeg_next as ffmpeg;

use std::{io, os::unix::prelude::OsStrExt, path::Path, time::Duration};

mod ffmpeg_backend;
mod input;
mod interface;
mod state;
mod util;

use clap::Parser;
use ffmpeg::ChannelLayout;
use interface::TVOptions;
use once_cell::sync::Lazy;
use question::Answer;
use tracing::*;
use tracing_subscriber::EnvFilter;

use crate::input::Stream;

static ARGS: Lazy<interface::Args> = Lazy::new(interface::Args::parse);

const EXEMPT_FILE_EXTENSIONS: [&str; 11] = [
    "clbin", "gif", "jpg", "md", "nfo", "png", "py", "rar", "sfv", "srr", "txt",
];

fn create_output_dir(path: &Path, tv_options: &Option<TVOptions>) -> io::Result<()> {
    let output_dir = if let Some(ref tv_options) = tv_options {
        path.join(format!("Season {:02}", tv_options.season))
    } else {
        path.join("newfiles")
    };

    if output_dir.is_dir() {
        info!(dir = ?output_dir, "Directory already exists");
    } else {
        std::fs::create_dir(&output_dir)?;
        info!(dir = ?output_dir, "Created directory");
    }

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    ffmpeg::init()?;

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    interface::validate_args(&ARGS);

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

    let path = ARGS.path.canonicalize()?;

    let entries = if path.is_file() {
        vec![path.clone()]
    } else if path.is_dir() {
        let mut v: Vec<_> = std::fs::read_dir(&path)?
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
    } else {
        error!("Provided path must be either a file or a directory");
        std::process::exit(1);
    };

    debug!(?entries);

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

    create_output_dir(&path, &tv_options)?;

    match ARGS.parallel {
        Some(jobs) => {
            let jobs = jobs.unwrap_or(usize::MAX);
            let mut running = Vec::new();
            'outer: loop {
                while running.len() < jobs && !commands.is_empty() {
                    let mut command = commands.pop().unwrap();
                    let child = command.spawn()?;
                    running.push(child);
                }

                if running.is_empty() {
                    break;
                }

                for i in 0..running.len() {
                    let child = match running.get_mut(i) {
                        Some(x) => x,
                        None => continue 'outer,
                    };
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
                std::thread::sleep(Duration::from_secs(10));
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
