use crate::TVOptions;
use std::path::Path;

pub fn generate_output_filename<P: AsRef<Path>>(path: P, tv_options: &TVOptions) -> String {
    let path = path.as_ref();
    if tv_options.enabled {
        return format!(
            "{} - s{:02}e{:02}.mkv",
            tv_options.title.as_ref().unwrap(),
            tv_options.season.unwrap(),
            tv_options.episode.unwrap()
        );
    } else {
        let input_filename = path.file_name().expect("Input filename is None").to_string_lossy();
        let input_ext = path.extension().expect("Input ext is None").to_string_lossy();
        let output_filename = input_filename.replace(input_ext.as_ref(), "mkv");
        return output_filename;
    }
}

