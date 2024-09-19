extern crate ffmpeg_the_third as ffmpeg;

use std::{
    collections::HashMap,
    error::Error,
    iter,
    os::unix::prelude::OsStrExt,
    path::{Path, PathBuf},
};

mod command;
mod directory;
mod input;
mod interface;
mod state;
mod tv;
mod util;

use clap::Parser;
use ffmpeg::ChannelLayout;
use once_cell::sync::Lazy;
use question::Answer;
use tokio::{process::Command, runtime::Runtime, task::JoinSet};
use tracing::*;
use tracing_subscriber::EnvFilter;
use tv::TVOptions;
use walkdir::WalkDir;

use crate::{command::CommandError, directory::OutputDir, input::Stream};

static ARGS: Lazy<interface::Args> = Lazy::new(interface::Args::parse);

const EXEMPT_FILE_EXTENSIONS: [&str; 12] = [
    "clbin", "gif", "jpg", "md", "nfo", "png", "py", "rar", "sfv", "srr", "txt", "srt",
];
const SUBTITLE_EXTS: [&str; 3] = ["ass", "srt", "srr"];

fn main() -> Result<(), Box<dyn Error>> {
    ffmpeg::init()?;

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    ARGS.validate();

    debug!(?ARGS);

    // Shut libav* up
    // Safety: No other threads exist, mutating global state is fine.
    unsafe {
        ffmpeg::ffi::av_log_set_level(ffmpeg::ffi::AV_LOG_FATAL);
    }

    let mut tv_options = TVOptions::from_cli();

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
        let mut entries = Vec::new();

        // For each path provided:
        //  - If it's a file, add it unconditionally - the user knows what they're doing
        //  - If it's a directory, walk it and add all files in it, if they are compatible
        // `path` should never be a symlink, as we canonicalize it.
        for path in ARGS.path.iter().map(|x| x.canonicalize()) {
            let path = path?;

            if path.is_file() {
                entries.push(path);
            } else if path.is_dir() {
                let it = WalkDir::new(path)
                    .max_depth(ARGS.depth)
                    .into_iter()
                    .filter_map(|path| path.ok())
                    .map(|x| x.path().to_owned())
                    .filter(|x| x.is_file())
                    .filter(
                        |path| !path.file_name().unwrap(/* all files have names */).as_bytes().starts_with(b"."), // Remove files that have no filename (?)
                    )
                    .filter(|path| {
                        // Remove files with extensions that are exempt
                        let Some(file_extension) = path.extension().and_then(|x| x.to_str()) else {
                            return false;
                        }; // Remove filles with no extension

                        // Remove files of the form `*.r00`, `*.r01`, etc
                        let is_rar_segment = matches!(file_extension.strip_prefix('r'), Some(s) if s.chars().all(|c| c.is_ascii_digit()));

                        !(ARGS
                            .ignored_extensions
                            .iter()
                            .any(|ignored| file_extension.ends_with(ignored))
                            || EXEMPT_FILE_EXTENSIONS.contains(&file_extension)
                            || is_rar_segment)
                    });

                entries.extend(it);
            } else {
                error!("Provided path must be either a file or a directory");
                std::process::exit(1);
            };
        }
        entries.sort_unstable_by(|a, b| a.file_name().cmp(&b.file_name()));
        entries
    };
    debug!(?entries);

    let mut associated_subtitles: HashMap<&Path, Vec<PathBuf>> = HashMap::new();

    for path in &entries {
        let videofile_name = path.file_stem().unwrap().to_string_lossy();
        let dir = path.parent().ok_or("Shouldn't be /")?;
        for child in std::fs::read_dir(dir)? {
            let child = child?.path();
            if child.is_dir() {
                continue;
            }
            let name = child.file_stem().unwrap().to_string_lossy();
            if name.starts_with(&*videofile_name) {
                if let Some(ext) = child.extension().map(|x| x.to_string_lossy()) {
                    if SUBTITLE_EXTS.contains(&&*ext.to_lowercase()) {
                        associated_subtitles.entry(path).or_default().push(child);
                    }
                }
            }
        }
    }

    let mut commands = Vec::with_capacity(entries.len());

    let output_dir = OutputDir::new(
        // Path::new is free (a transmute). Clippy is wrong.
        #[allow(clippy::or_fun_call)]
        ARGS.output_path.as_deref().unwrap_or(Path::new(".")),
        &tv_options,
    );

    let mut errored_paths = Vec::new();

    for input_filepath in &entries {
        let associated_subs = {
            if let Some(v) = associated_subtitles.get(input_filepath.as_path()) {
                v.as_slice()
            } else {
                &[]
            }
        };

        let output_filename = command::generate_output_filename(input_filepath, &tv_options);
        let output_path = output_dir.0.join(output_filename);

        if let Some(ref mut tv_options) = tv_options {
            tv_options.episode += 1;
        }

        let file = ffmpeg::format::input(&input_filepath)?;

        let mut parsed = input::parse_stream_metadata(file, 0);

        for (i, path) in associated_subs.iter().enumerate() {
            let file = ffmpeg::format::input(path)?;
            parsed.extend_from_slice(&input::parse_stream_metadata(file, i + 1));
        }

        let stream_mappings = input::get_stream_mappings(&parsed);
        let codec_mappings = input::get_codec_mapping(&stream_mappings);

        let mappings = &stream_mappings;
        let codecs = &codec_mappings;

        if mappings.video.is_empty() {
            error!("No video streams found");
            std::process::exit(1);
        }

        println!(
            "Input file '{}' -> '{}':",
            input_filepath.display(),
            output_path.display(),
        );
        for stream in mappings.iter() {
            let file = stream.file();
            let index = stream.index();
            let codec = codecs.get(&index).unwrap();
            let oldcodec = stream.codec();
            let newcodec = match codec {
                None => &oldcodec,
                Some(x) => x,
            };

            print!("Mapping stream {file}:{index}: {oldcodec:?} ");

            if let Some(title) = stream
                .as_audio()
                .map(|x| x.title.as_deref())
                .or_else(|| stream.as_subtitle().map(|x| x.title.as_deref()))
            {
                print!("'{}' ", title.unwrap_or("[untitled]"));
            }

            if let Stream::Audio(audio) = stream {
                let layout = ChannelLayout::from(&audio.channel_layout);
                if layout == ChannelLayout::STEREO {
                    print!("(2.0) ");
                } else if layout == ChannelLayout::_5POINT1 {
                    print!("(5.1) ");
                } else if layout == ChannelLayout::_7POINT1 {
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
        let dropped_audio =
            parsed.iter().filter(|&x| x.as_audio().is_some()).count() - mappings.audio.len();
        let dropped_subs =
            parsed.iter().filter(|&x| x.as_subtitle().is_some()).count() - mappings.subtitle.len();
        println!("Dropping {dropped_audio} audio streams and {dropped_subs} subtitle streams",);

        let command = command::generate_ffmpeg_command(
            input_filepath,
            associated_subs,
            &output_path,
            stream_mappings,
            codec_mappings,
        );

        info!(?command);
        match command {
            Ok(command) => commands.push(command),
            Err(CommandError::FileExists) => {
                if !ARGS.continue_processing {
                    std::process::exit(1);
                }
                errored_paths.push(input_filepath);
                continue;
            }
        }
    }

    if ARGS.print_commands {
        for command in &commands {
            let command = command.as_std();
            let cmd = iter::once(command.get_program())
                .chain(command.get_args())
                .map(|x| x.to_string_lossy())
                .map(shell_escape::escape)
                .collect::<Vec<_>>()
                .join(" ");
            println!("{}", cmd);
        }
        println!();
    }

    if ARGS.simulate {
        eprintln!("Simulate mode; not executing commands");
        return Ok(());
    }

    if ARGS.yes || !util::confirm("Continue?", Some(Answer::YES)) {
        eprintln!("Aborting");
        return Ok(());
    }

    output_dir.create().unwrap();

    let rt = Runtime::new()?;
    rt.block_on(run_commands(commands))?;

    if !errored_paths.is_empty() {
        eprintln!("Errors occured in {} paths:", errored_paths.len());
        for p in errored_paths {
            eprintln!("  {}", p.display());
        }
    }

    Ok(())
}

async fn run_commands(commands: Vec<Command>) -> Result<(), Box<dyn Error>> {
    if !ARGS.parallel {
        for (i, mut command) in commands.into_iter().enumerate() {
            let mut handle = command.spawn()?;
            let status = handle.wait().await?;
            if !status.success() {
                error!(
                    "Command {i} failed with status code {}",
                    status.code().unwrap()
                );
            }
        }
        return Ok(());
    }

    let mut js = JoinSet::new();

    for mut command in commands {
        let handle = command.spawn()?;
        js.spawn(async move {
            let mut handle = handle;
            handle.wait().await
        });
    }

    while let Some(status) = js.join_next().await {
        let status = status??;
        if !status.success() {
            error!("Command failed with status code {}", status.code().unwrap());
        }
    }

    Ok(())
}
