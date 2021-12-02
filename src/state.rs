use crate::interface::TVOptions;
use crate::ARGS;

use std::{
    fs::File,
    io::{BufRead, Write},
    path::Path,
};

use tracing::*;

pub fn write_state(tv_options: &TVOptions) -> std::io::Result<()> {
    let mut file = File::create(&ARGS.statefile)?;
    write!(
        &mut file,
        "{}\n{}\n{}",
        tv_options.title, tv_options.season, tv_options.episode,
    )
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

    let reader = std::io::BufReader::new(file);
    let mut lines = match reader
        .lines()
        .collect::<Result<Vec<String>, std::io::Error>>()
    {
        Ok(x) => x,
        Err(e) => {
            warn!(err = ?e, "Failed to read file");
            return None;
        }
    };

    if !validate_state(&lines) {
        return None;
    }

    Some(TVOptions {
        title: std::mem::take(&mut lines[0]),
        season: lines[1].parse::<usize>().ok()?,
        episode: lines[2].parse::<usize>().ok()?,
    })
}

fn validate_state(statefile: &[String]) -> bool {
    if statefile.len() != 3 {
        return false;
    }

    if statefile.iter().any(|l| l.is_empty()) {
        return false;
    }

    if statefile
        .iter()
        .skip(1)
        .any(|l| l.chars().any(|x| !x.is_digit(10)))
    {
        return false;
    }

    true
}
