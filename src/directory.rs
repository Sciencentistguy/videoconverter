use std::{io, path::PathBuf};

use color_eyre::eyre::{Context, Result};
use tracing::info;

use crate::{ARGS, tv::TVOptions};

pub struct OutputDir(pub PathBuf);

impl OutputDir {
    pub fn new(tv_options: &Option<TVOptions>, rename_title: &Option<String>) -> Result<Self> {
        let current_dir =
            || std::env::current_dir().wrap_err("Failed to determine the current directory");

        Ok(Self(
            if let Some(TVOptions { title, season, .. }) = tv_options {
                let mut path = match ARGS.tv_output_prefix.as_deref() {
                    // prefix + tv title
                    Some(prefix) => prefix.join(title),
                    None => current_dir()?,
                };

                path.push(format!("Season {:02}", season));
                path
            } else if let Some(output_prefix) = ARGS.movie_output_dir.as_deref()
                && rename_title.is_some()
            {
                output_prefix.to_owned()
            } else {
                let mut base_path = current_dir()?;
                base_path.push("newfiles");
                base_path
            },
        ))
    }

    pub fn create(&self) -> io::Result<()> {
        if self.0.is_dir() {
            info!(dir = ?self.0, "Directory already exists");
        } else {
            std::fs::create_dir_all(&self.0)?;
            info!(dir = ?self.0, "Created directory");
        }

        Ok(())
    }
}
