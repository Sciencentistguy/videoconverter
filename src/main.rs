extern crate ffmpeg_next as ffmpeg;
extern crate regex;

#[macro_use]
extern crate lazy_static;

mod frontend;
mod interface;
mod util;
mod backend;

use interface::TVOptions;
use itertools::sorted;
use log::{debug, info};
use regex::Regex;
use structopt::StructOpt;
use frontend::StreamType;
use std::collections::HashMap;
use ffmpeg::codec;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    ffmpeg::init()?;
    pretty_env_logger::init();

    lazy_static! {
        static ref EXEMPT_EXTENSION_REGEX: Regex = Regex::new(r"r\d+").unwrap();
    }

    let opt = interface::Opt::from_args();

    debug!("{:?}", opt);

    // Squelch libav* errors
    unsafe {
        ffmpeg::ffi::av_log_set_level(ffmpeg::ffi::AV_LOG_FATAL);
    }

    let mut tv_options = interface::get_tv_options()?;

    debug!(
        "tv_mode: {}, tv_show_title: {:?}, tv_show_season: {:?}, tv_show_episode: {:?}.",
        tv_options.enabled, tv_options.title, tv_options.season, tv_options.episode
    );

    let entries = sorted(
        std::fs::read_dir(&opt.path)?
            .map(|entry| entry.unwrap().path())
            .filter(|path| !path.is_dir()) // Remove directories
            .filter(|path| {
                // Remove files that start with '.'
                let filename = path.file_name().and_then(|x| x.to_str()).unwrap();
                filename.chars().nth(0).unwrap() != '.'
            })
            .filter(|path| {
                // Remove files with extensions that are exempt
                let file_extension = match path.extension().and_then(|x| x.to_str()) {
                    Some(x) => x,
                    None => {
                        return false;
                    }
                };
                let exempt_file_extensions = ["gif", "jpg", "md", "nfo", "png", "py", "rar", "sfv", "srr", "txt"];
                return !(exempt_file_extensions.contains(&file_extension) || EXEMPT_EXTENSION_REGEX.is_match(file_extension));
            }),
    );

    // prepare directory
    {
        let dir_to_make = if tv_options.enabled {
            opt.path.join(format!("Season {:02}", tv_options.season.unwrap()))
        } else {
            opt.path.join("newfiles")
        };
        let dir_as_str: &str = dir_to_make.as_os_str().to_str().expect("Path contained invalid unicode.");

        if dir_to_make.is_dir() {
            info!("Directory '{}' already exists.", dir_as_str);
        } else {
            if opt.simulate {
                info!("Simulate mode: not creating directory '{}'", dir_as_str);
            } else {
                std::fs::create_dir(&dir_to_make)?;
                info!("Created directory '{}'.", dir_as_str);
            }
        }
    }

    for input_path in entries {
        let output_filename = backend::generate_output_filename(&input_path, &tv_options);

        let output_path = if tv_options.enabled {
            input_path
                .parent()
                .expect("Somehow the input_path was root")
                .join(format!("Season {:02}", tv_options.season.unwrap()))
        } else {
            input_path.parent().expect("Somehow the input_path was root").join("newfiles")
        }
        .join(output_filename);

        info!(
            "Mapping '{}' --> '{}'",
            input_path.as_os_str().to_str().expect("Path contained invalid unicode."),
            output_path.as_os_str().to_str().expect("Path contained invalid unicode.")
        );

        let file = ffmpeg::format::input(&input_path)?;

        let parsed = frontend::parse_stream_metadata(&file);
        let mappings = frontend::get_mappings(&parsed);
        let codecs = frontend::get_codecs(&parsed, &mappings);
        log_codec_mapping(&parsed, &mappings, &codecs);
        if let Some(ref mut e) = tv_options.episode {
            *e += 1;
        }
    }

    return Ok(());
}

fn log_codec_mapping(parsed: &[StreamType], mappings: &[usize], codecs: &HashMap<usize, Option<codec::Id>>) {
    for index in mappings {
        let codec = codecs.get(&index).unwrap();
        let oldcodec = match &parsed[*index] {
            StreamType::Video(video) => &video.codec,
            StreamType::Audio(audio) => &audio.codec,
            StreamType::Subtitle(subtitle) => &subtitle.codec,
        };
        let newcodec = match codec {
            None => &oldcodec,
            Some(x) => x,
        };
        info!(
            "Mapping stream {}: {:?} -> {:?}{}",
            index,
            oldcodec,
            newcodec,
            if codec.is_none() { " (copy)" } else { "" }
        );
    }
}
