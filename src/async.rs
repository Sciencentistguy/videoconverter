use std::{
    path::Path,
    process::{ExitStatus, Stdio},
    sync::Arc,
    time::Duration,
};

use color_eyre::eyre::eyre;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use tokio::{
    io::{AsyncBufReadExt as _, BufReader},
    process::Child,
    signal,
    sync::{Semaphore, broadcast},
    task::JoinSet,
};
use tracing::{error, warn};

use crate::{ARGS, Command, Result};

#[derive(Debug, thiserror::Error)]
#[error("interrupted")]
pub struct Interrupted;

pub async fn run_commands(commands: Vec<Command>) -> Result<()> {
    let count = match ARGS.parallel {
        None => num_cpus::get(),
        Some(x) => {
            // ensure we don't create more processes than cores
            std::cmp::min(x, num_cpus::get())
        }
    };

    let sem = Arc::new(Semaphore::new(count));
    let mut js = JoinSet::new();

    let mpb = MultiProgress::new();
    let (tx, _) = broadcast::channel(1);

    // An overall progress bar that counts how many transcodes have completed.
    // We hide this if all the encodes are happening at once
    let overall_pb = if commands.len() != count {
        let overall_pb = mpb.add(ProgressBar::new(commands.len() as _));
        overall_pb.set_style(
            ProgressStyle::with_template(
                "[{elapsed}] Overall Progress: {wide_bar:.cyan/blue} ({pos}/{len})",
            )?
            .progress_chars("=>-"),
        );
        overall_pb.enable_steady_tick(Duration::from_secs(1));
        Some(overall_pb)
    } else {
        None
    };

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
             Err(Interrupted.into())
        }
        ret = async {
            let mut failed = 0;
            for Command {
                inner: mut command,
                length,
                filename,
            } in commands
            {
                let permit = sem.clone().acquire_owned().await?;
                command.arg("-progress");
                command.arg("pipe:1");
                command.stdout(Stdio::piped());
                command.stderr(Stdio::null());
                // Should this task end early, its ffmpeg must not carry on writing unseen.
                command.kill_on_drop(true);

                // An existing output is one the user chose to overwrite, which ffmpeg may not have
                // touched before failing, so only outputs this run creates are removed.
                let existed = filename.exists();
                let handle = match command.spawn() {
                    Ok(handle) => handle,
                    Err(err) => {
                        error!(file = ?filename, %err, "Failed to start ffmpeg");
                        failed += 1;
                        continue;
                    }
                };
                let mpb = mpb.clone();
                let overall_pb = overall_pb.clone();
                let mut rx = tx.subscribe();

                js.spawn(async move {
                    let result = convert(handle, length, &filename, &mpb, overall_pb.as_ref(), &mut rx).await;
                    // Only finished outputs are left behind, so callers can trust them
                    if !existed && !matches!(&result, Ok(status) if status.success()) {
                        let _ = tokio::fs::remove_file(&filename).await;
                    }
                    drop(permit);
                    result
                });
            }

            while let Some(joined) = js.join_next().await {
                match joined {
                    Ok(Ok(status)) if status.success() => {}
                    Ok(Ok(status)) => {
                        let msg = match status.code() {
                            Some(234) => "usually caused by a problematic stream in the source file",
                            _ => "unknown"
                        };
                        error!("Command failed with status code {}: {}", status.code().unwrap_or(-1), msg);
                        failed += 1;
                    }
                    Ok(Err(err)) => {
                        error!("Command failed: {err}");
                        failed += 1;
                    }
                    Err(err) => {
                        error!("Command task failed: {err}");
                        failed += 1;
                    }
                }
            }
            if failed > 0 {
                return Err(eyre!("{failed} command(s) failed"));
            }
            Ok(())
        } => ret,
    }
}

/// Follows one ffmpeg process to completion, showing its progress, unless cancelled through `rx`.
async fn convert(
    mut handle: Child,
    length: Duration,
    filename: &Path,
    mpb: &MultiProgress,
    overall_pb: Option<&ProgressBar>,
    rx: &mut broadcast::Receiver<()>,
) -> Result<ExitStatus> {
    let stdout = handle
        .stdout
        .take()
        .ok_or_else(|| eyre!("ffmpeg stdout was not captured"))?;
    let reader = BufReader::new(stdout);
    let mut lines = reader.lines();

    let pb = ProgressBar::new(length.as_micros() as _);
    if let Some(overall_pb) = overall_pb {
        mpb.insert_before(overall_pb, pb.clone());
    } else {
        mpb.add(pb.clone());
    }

    pb.set_style(
        ProgressStyle::with_template(
            "[{elapsed}] {msg} {wide_bar:.cyan/blue} {percent_precise:>6}% (eta: {eta})",
        )?
        .progress_chars("=>-"),
    );
    pb.set_message(
        filename
            .file_name()
            .ok_or_else(|| eyre!("Output path has no filename"))?
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
                return Err(eyre!("Cancelled"));
            }
        }
    }

    tokio::select! {
        ret = handle.wait() => {
            overall_pb.inspect(|pb| pb.inc(1));
            pb.finish_and_clear();
            ret.map_err(|e| e.into())
        }
        _ = rx.recv() => {
            let _ = handle.kill().await;
            let _ = handle.wait().await;
            pb.finish_and_clear();
            Err(eyre!("Cancelled"))
        }
    }
}
