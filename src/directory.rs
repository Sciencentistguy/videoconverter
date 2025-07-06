use std::{
    io,
    path::{Path, PathBuf},
};

use tracing::info;

use crate::tv::TVOptions;

pub struct OutputDir(pub PathBuf);

impl OutputDir {
    pub fn new(path: &Path, tv_options: &Option<TVOptions>) -> Self {
        let output_dir = if let Some(tv_options) = tv_options {
            path.join(format!("Season {:02}", tv_options.season))
        } else {
            path.join("newfiles")
        };
        Self(output_dir)
    }

    pub fn create(&self) -> io::Result<()> {
        if self.0.is_dir() {
            info!(dir = ?self.0, "Directory already exists");
        } else {
            std::fs::create_dir(&self.0)?;
            info!(dir = ?self.0, "Created directory");
        }

        Ok(())
    }
}
