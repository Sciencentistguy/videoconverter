use std::{
    io,
    path::{Path, PathBuf},
};

use tracing::info;

use crate::{ARGS, tv::TVOptions};

pub struct OutputDir(pub PathBuf);

impl OutputDir {
    pub fn new(tv_options: &Option<TVOptions>) -> Self {
        // let path = match ARGS
        // .output_prefix
        // .as_deref() {
        // None => ARGS.output_path.as_deref().unwrap_or(Path::new(".")),
        // Some(prefix)
        // };

        Self(if let Some(TVOptions { title, season, .. }) = tv_options {
            let mut path = ARGS
                .output_prefix
                .as_deref()
                .map(|prefix| {
                    // prefix + tv title
                    prefix.join(title)
                })
                .unwrap_or_else(|| Path::new(".").to_owned());

            path.push(format!("Season {:02}", season));
            path
        } else {
            Path::new("./newfiles").to_owned()
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
