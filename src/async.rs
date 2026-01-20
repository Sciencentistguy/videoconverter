use std::{process::Stdio, sync::Arc, time::Duration};

use color_eyre::eyre::eyre;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use tokio::{
    io::{AsyncBufReadExt as _, BufReader},
    signal,
    sync::{Semaphore, broadcast},
    task::JoinSet,
};
use tracing::{error, warn};

use crate::{ARGS, Command, Result};

pub async fn run_commands(commands: Vec<Command>) -> Result<()> {
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

    // An overall progress bar that counts how many transcodes have completed.
    // We hide this if all the encodes are happening at once
    let overall_pb = if commands.len() != count {
        let overall_pb = mpb.add(ProgressBar::new(commands.len() as _));
        overall_pb.set_style(
            ProgressStyle::with_template(
                "[{elapsed}] Overall Progress: {wide_bar:.cyan/blue} ({pos}/{len})",
            )
            .unwrap()
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
                command.stdout(Stdio::piped());
                command.stderr(Stdio::null());

                // move closures be like
                let handle = command.spawn()?;
                let mpb = mpb.clone();
                let overall_pb = overall_pb.clone();
                let mut rx = tx.subscribe();

                js.spawn(async move {
                    let mut handle = handle;
                    let stdout = handle.stdout.take().unwrap();
                    let reader = BufReader::new(stdout);
                    let mut lines = reader.lines();

                    let pb = ProgressBar::new(length.as_micros() as _);
                    if let Some(overall_pb) = &overall_pb {
                        mpb.insert_before(overall_pb, pb.clone());
                    } else {
                        mpb.add(pb.clone());
                    }

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
                            overall_pb.inspect(|pb| pb.inc(1));
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
