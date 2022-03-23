use std::fmt::Display;
use std::path::PathBuf;

use crate::state;
use crate::util;
use crate::ARGS;

use clap::ArgEnum;
use clap::Parser;
use question::Answer;
use regex::Regex;

#[derive(Parser, Debug)]
#[clap(name = "videoconverter", version, author)]
pub struct Args {
    /// Keep all streams, regardless of language metadata.
    #[clap(short, long)]
    pub all_streams: bool,

    /// Specify a CRF value to be passed to libx264
    #[clap(long, default_value = "20")]
    pub crf: u8,

    /// Specify a crop filter. These are of the format `crop=height:width:x:y`
    #[clap(long, parse(try_from_str = parse_crop_filter))]
    pub crop: Option<String>,

    /// Force deinterlacing of video
    #[clap(short = 'd', long, conflicts_with = "no-deinterlace")]
    pub force_deinterlace: bool,

    /// Disable automatic deinterlacing of video
    #[clap(short = 'D', long, conflicts_with = "force-deinterlace")]
    pub no_deinterlace: bool,

    /// Force reencoding of video
    #[clap(long = "force-reencode")]
    pub force_reencode_video: bool,

    /// Specify encoder to use.
    #[clap(short, long, default_value = "Libx264", ignore_case = true, arg_enum)]
    pub encoder: VideoEncoder,

    /// Specify encoder preset
    #[clap(long, default_value = "Slow", ignore_case = true, arg_enum)]
    pub preset: VideoEncoderPreset,

    /// Disable hardware-accelerated decoding
    #[clap(long)]
    pub no_hwaccel: bool,

    /// Do not actually perform the conversion
    #[clap(short, long)]
    pub simulate: bool,

    /// Specify libx264 tune. Has no effect with Nvenc.
    #[clap(short, long, ignore_case = true, arg_enum)]
    pub tune: Option<Libx264Tune>,

    /// The path to operate on
    #[clap(default_value = ".")]
    pub path: std::path::PathBuf,

    /// Enables renaming of files to TV show format
    #[clap(long, short = 'T')]
    pub tv_mode: bool,

    /// The path for the statefile
    #[clap(long, default_value = "/tmp/videoconverter.state")]
    pub statefile: PathBuf,

    /// Spawn each ffmpeg command concurrently. WARNING: Currently doesn't kill child processes
    /// properly, and so cannot be safely interrupted with, e.g. Ctrl-C.
    #[clap(short, long)]
    pub parallel: bool,

    /// Sets the default language to the first stream with the given language code.
    #[clap(long, value_name = "language")]
    pub default_audio_language: Option<String>,
}

fn parse_crop_filter(input: &str) -> Result<String, String> {
    let r = Regex::new(r"crop=\d\+:\d\+:\d\+:\d\+").unwrap();
    if !r.is_match(input) {
        return Err("must be of the form `crop=height:width:x:y`".to_string());
    }
    // TODO check that its a proper crop string
    Ok(input.to_owned())
}

#[derive(Debug, ArgEnum, Clone)]
pub enum VideoEncoder {
    Libx264,
    Libx265,
    Nvenc,
}

impl Display for VideoEncoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, ArgEnum, Clone)]
pub enum VideoEncoderPreset {
    Ultrafast,
    Superfast,
    Veryfast,
    Faster,
    Fast,
    Medium,
    Slow,
    Slower,
    Veryslow,
    Placebo,
}

impl Display for VideoEncoderPreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, ArgEnum, Clone)]
pub enum Libx264Tune {
    Film,
    Animation,
    Grain,
    StillImage,
    Psnr,
    Ssim,
    FastDecode,
    ZeroLatency,
}

impl Display for Libx264Tune {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug)]
pub struct TVOptions {
    pub title: String,
    pub season: usize,
    pub episode: usize,
}

pub fn get_tv_options() -> Option<TVOptions> {
    let enabled = ARGS.tv_mode || util::confirm("TV Show Mode", Some(Answer::NO));
    if !enabled {
        return None;
    }

    let mut previous_state = state::read_state();
    let mut title = String::new();

    if let Some(ref mut previous_state) = previous_state {
        let use_old_value = util::confirm(
            &format!("Use previous title? ({})", previous_state.title),
            None,
        );

        if use_old_value {
            title = std::mem::take(&mut previous_state.title);
        }
    }

    if title.is_empty() {
        title = loop {
            let response = util::prompt("Please enter the title of the TV show");
            if !response.is_empty() {
                break response;
            }
        }
    }

    let mut season = None;

    if let Some(previous_state) = previous_state {
        let use_old_value = util::confirm(
            &format!("Use previous season? ({})", previous_state.season),
            None,
        );

        if use_old_value {
            season = Some(previous_state.season);
        }
    }

    if season.is_none() {
        season = loop {
            match util::prompt("Enter the season index of the tv show").parse::<usize>() {
                Ok(x) => break Some(x),
                Err(_) => {
                    println!("Invalid response. Please try again.");
                    continue;
                }
            }
        }
    }

    let episode = loop {
        if let Ok(x) =
            util::prompt("Enter the index of the first episode in this directory").parse::<usize>()
        {
            break x;
        }
    };

    Some(TVOptions {
        title,
        season: season.unwrap(),
        episode,
    })
}
