use crate::tv::TVOptions;
use crate::ARGS;

use std::{fs::File, path::Path};

use strum::Display;
use thiserror::Error;
use tracing::*;

#[derive(Error, Debug, Display)]
pub enum StateErr {
    IO(#[from] std::io::Error),
    Serde(#[from] serde_json::Error),
}

pub fn write_state(tv_options: &TVOptions) -> Result<(), StateErr> {
    let file = File::create(&ARGS.statefile)?;
    serde_json::to_writer(file, tv_options)?;
    Ok(())
}

pub fn read_state() -> Option<TVOptions> {
    if !Path::new(&ARGS.statefile).exists() {
        return None;
    }

    let file = match File::open(&ARGS.statefile) {
        Ok(x) => x,
        Err(e) => {
            warn!(err = ?e, "Failed to open file");
            return None;
        }
    };

    serde_json::from_reader(file).ok()
}
