extern crate ffmpeg_the_third as ffmpeg;

use std::{
    collections::HashMap,
    io::ErrorKind,
    iter,
    os::unix::prelude::OsStrExt,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

mod command;
mod directory;
mod input;
mod interface;
mod state;
mod tv;
mod util;

use clap::Parser;
use color_eyre::eyre::{Context, Result, eyre};
use ffmpeg::ChannelLayout;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use once_cell::sync::Lazy;
use question::Answer;
use tokio::{
    runtime::Runtime,
    signal,
    sync::{Semaphore, broadcast},
    task::JoinSet,
};
use tracing::*;
use tracing_subscriber::EnvFilter;
use tv::TVOptions;
use walkdir::WalkDir;

use crate::{command::CommandError, directory::OutputDir, input::Stream, state::Db};

static ARGS: Lazy<interface::Args> = Lazy::new(interface::Args::parse);

const EXEMPT_FILE_EXTENSIONS: [&str; 12] = [
    "clbin", "gif", "jpg", "md", "nfo", "png", "py", "rar", "sfv", "srr", "txt", "srt",
];
const SUBTITLE_EXTS: [&str; 2] = ["ass", "srt"];

fn main() -> Result<()> {
    color_eyre::install()?;
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

    let entries = {
        let mut entries = Vec::new();

        // For each path provided:
        //  - If it's a file, add it unconditionally - the user knows what they're doing
        //  - If it's a directory, walk it and add all files in it, if they are compatible
        // `path` should never be a symlink, as we canonicalize it.
        for path in ARGS.path.iter() {
            let path = match path.canonicalize() {
                Ok(path) => path,
                Err(e) if e.kind() == ErrorKind::NotFound => {
                    eprintln!("ERROR: File {} not found", path.display());
                    continue;
                }
                Err(e) => {
                    return Err(eyre!(
                        "Failed to canonicalize path '{}': {}",
                        path.display(),
                        e
                    ));
                }
            };

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

                        if ARGS
                            .ignored_extensions
                            .iter()
                            .any(|ignored| file_extension.ends_with(ignored))
                        {
                            return false;
                        }

                        if  EXEMPT_FILE_EXTENSIONS.contains(&file_extension) {
                            return false;
                        }

                        if is_rar_segment {
                            return false;
                        }

                        true
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

    let title = entries
        .first()
        .and_then(|x| x.file_name().map(|y| y.to_string_lossy()));

    let db = Db::new().unwrap();

    let mut tv_options = TVOptions::from_cli(&db, title.as_deref());

    if let Some(ref tv_options) = tv_options {
        db.write(tv_options);
    }

    debug!(?tv_options);

    debug!(?entries);

    let mut associated_subtitles: HashMap<&Path, Vec<PathBuf>> = HashMap::new();

    for path in &entries {
        let videofile_name = path.file_stem().unwrap().to_string_lossy();
        let dir = path.parent().ok_or_else(|| eyre!("Shouldn't be /"))?;
        for child in std::fs::read_dir(dir)? {
            let child = child?.path();
            if child.is_dir() {
                continue;
            }
            let name = child.file_stem().unwrap().to_string_lossy();
            if name.starts_with(&*videofile_name)
                && let Some(ext) = child.extension().map(|x| x.to_string_lossy())
                && SUBTITLE_EXTS.contains(&&*ext.to_lowercase())
            {
                debug!(video_path=?path, subtitle_path=?child, "Found associated subtitle");
                associated_subtitles.entry(path).or_default().push(child);
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

        let file = ffmpeg::format::input(&input_filepath)
            .wrap_err_with(|| format!("Filepath: {}", input_filepath.display()))?;

        let mut parsed = input::parse_stream_metadata(file, 0);

        for (i, path) in associated_subs.iter().enumerate() {
            let file = ffmpeg::format::input(path)
                .wrap_err_with(|| format!("Filepath: {}", path.display()))?;
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
        let length = input::length(input_filepath);

        info!(?command);
        match command {
            Ok(command) => commands.push(Command {
                inner: command,
                length,
                filename: output_path,
            }),
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
            let command = command.inner.as_std();
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

async fn run_commands(commands: Vec<Command>) -> Result<()> {
    let count = match ARGS.parallel {
        None => 1,
        Some(None) => num_cpus::get(),
        Some(Some(x)) => {
            // ensure we don't create more processes than cores
            std::cmp::min(x, num_cpus::get())
        }
    };

    let sem = Arc::new(Semaphore::new(count));
    let mut js = JoinSet::new();

    let mpb = MultiProgress::new();
    let (tx, _) = broadcast::channel(1);
    let overall_pb = mpb.add(ProgressBar::new(commands.len() as _));
    overall_pb.set_style(
        ProgressStyle::with_template(
            "[{elapsed}] Overall Progress: {wide_bar:.cyan/blue} ({pos}/{len})",
        )
        .unwrap()
        .progress_chars("=>-"),
    );
    overall_pb.tick();
    // While the commands are running, the ctrl-c handler is also running.
    // If it receives a ctrl-c, it sends a message to all running tasks to cancel.
    // Each command task polls both its own work, and the cancel message, and if it
    // receives the cancel message, it kills its ffmpeg process and exits.
    // This also results in any remaining commands never being started.
    tokio::select! {
        _ = signal::ctrl_c() => {
             eprintln!("\nCtrl-C received, stopping...");
             let _ = tx.send(());
             while js.join_next().await.is_some() {}
             Ok(())
        }
        ret = async {
            for Command {
                inner: mut command,
                length,
                filename,
            } in commands
            {
                let permit = sem.clone().acquire_owned().await?;
                command.arg("-progress");
                command.arg("pipe:1");
                command.stdout(std::process::Stdio::piped());
                command.stderr(std::process::Stdio::null());

                // move closures be like
                let handle = command.spawn()?;
                let mpb = mpb.clone();
                let overall_pb = overall_pb.clone();
                let mut rx = tx.subscribe();

                js.spawn(async move {
                    let mut handle = handle;
                    let stdout = handle.stdout.take().unwrap();
                    let reader = tokio::io::BufReader::new(stdout);
                    let mut lines = tokio::io::AsyncBufReadExt::lines(reader);

                    let pb = mpb.insert_before(&overall_pb ,ProgressBar::new(length.as_micros() as _));
                    pb.set_style(
                        ProgressStyle::with_template(
                            "[{elapsed}] {msg} {wide_bar:.cyan/blue} {percent_precise:>6}% (eta: {eta})",
                        )
                        .unwrap()
                        .progress_chars("=>-"),
                    );
                    pb.set_message(
                        filename
                            .file_name()
                            .expect("Output should always have a name")
                            .to_string_lossy()
                            .to_string(),
                    );

                    loop {
                        tokio::select! {
                            val = lines.next_line() => {
                                match val {
                                    Ok(Some(line)) => {
                                        if let Some("end") = line.strip_prefix("progress=") {
                                            break;
                                        }
                                        if let Some(us) = line.strip_prefix("out_time_us=") {
                                            let Ok(us) = us.parse() else {
                                                warn!("Failed to parse out_time_us value: {}", us);
                                                continue;
                                            };
                                            let dur = Duration::from_micros(us);
                                            pb.set_position(dur.as_micros() as u64);
                                        }
                                    }
                                    _ => break,
                                }
                            }
                            _ = rx.recv() => {
                                let _ = handle.kill().await;
                                let _ = handle.wait().await;
                                pb.finish_and_clear();
                                drop(permit);
                                return Err(eyre!("Cancelled"));
                            }
                        }
                    }

                    tokio::select! {
                        ret = handle.wait() => {
                            overall_pb.inc(1);
                            pb.finish_and_clear();
                            drop(permit);
                            ret.map_err(|e| e.into())
                        }
                        _ = rx.recv() => {
                            let _ = handle.kill().await;
                            let _ = handle.wait().await;
                            pb.finish_and_clear();
                            drop(permit);
                            Err(eyre!("Cancelled"))
                        }
                    }
                });
            }

            while let Some(status) = js.join_next().await {
                let status = status??;
                if !status.success() {
                    error!("Command failed with status code {}", status.code().unwrap_or(-1));
                }
            }
            Ok(())
        } => ret,
    }
}

struct Command {
    inner: tokio::process::Command,
    length: Duration,
    filename: PathBuf,
}
