use std::{io, path::PathBuf};

use tracing::info;

use crate::{ARGS, tv::TVOptions};

pub struct OutputDir(pub PathBuf);

impl OutputDir {
    pub fn new(tv_options: &Option<TVOptions>, rename_title: &Option<String>) -> Self {
        Self(if let Some(TVOptions { title, season, .. }) = tv_options {
            let mut path = ARGS
                .output_prefix
                .as_deref()
                .map(|prefix| {
                    // prefix + tv title
                    prefix.join(title)
                })
                .unwrap_or_else(|| {
                    std::env::current_dir().expect("Current working directory should exist")
                });

            path.push(format!("Season {:02}", season));
            path
        } else if let Some(output_prefix) = ARGS.output_prefix.as_deref()
            && rename_title.is_some()
        {
            output_prefix.to_owned()
        } else {
            let mut base_path =
                std::env::current_dir().expect("Current working directory should exist");
            base_path.push("newfiles");
            base_path
        })
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
