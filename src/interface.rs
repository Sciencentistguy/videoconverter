use std::ops::Deref;
use std::path::PathBuf;
use std::str::FromStr;

use crate::state;
use crate::util;
use crate::ARGS;

use clap::Parser;
use clap::ValueEnum;
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
    #[clap(long)]
    pub crop: Option<CropFilter>,

    /// Disable automatic deinterlacing of video
    #[clap(short = 'D', long)]
    pub no_deinterlace: bool,

    /// Force deinterlacing of video
    #[clap(short = 'd', long, conflicts_with = "no_deinterlace")]
    pub force_deinterlace: bool,

    /// Force reencoding of video
    #[clap(long = "force-reencode")]
    pub force_reencode_video: bool,

    /// Disable reencoding of video
    #[clap(long, conflicts_with_all = &["force_reencode_video", "force_deinterlace"])]
    pub copy_video: bool,

    /// Enable reencoding of audio
    #[clap(long)]
    pub reencode_audio: bool,

    /// Enable reencoding of subtitles
    #[clap(long)]
    pub reencode_subs: bool,

    /// Specify encoder to use.
    #[clap(short, long, default_value = "libx264", ignore_case = true, value_enum)]
    pub encoder: VideoEncoder,

    /// Specify encoder preset
    #[clap(long, default_value = "slow", ignore_case = true, value_enum)]
    pub preset: VideoEncoderPreset,

    /// Disable hardware-accelerated decoding
    #[clap(long)]
    pub no_hwaccel: bool,

    /// Do not actually perform the conversion
    #[clap(short, long)]
    pub simulate: bool,

    /// Specify libx264 tune. Has no effect with Nvenc.
    #[clap(short, long, ignore_case = true, value_enum)]
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

    /// Spawn each ffmpeg command concurrently.
    #[clap(short, long, value_name = "MAX_JOBS")]
    pub parallel: Option<Option<usize>>,

    /// Sets the default language to the first stream with the given language code.
    #[clap(long, value_name = "language")]
    pub default_audio_language: Option<String>,

    /// Weights file for nnedi3 deinterlace filter
    #[clap(long, default_value = "~/.ffmpeg/nnedi3_weights")]
    pub nnedi_weights: String,

    /// Overwrite output file, instead of erroring out.
    #[clap(long)]
    pub overwrite: bool,
}

#[derive(Debug, Clone)]
pub struct CropFilter(pub String);

impl Deref for CropFilter {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromStr for CropFilter {
    type Err = &'static str;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let r = Regex::new(r"crop=\d\+:\d\+:\d\+:\d\+").unwrap();
        if !r.is_match(input) {
            return Err("Crop filter must be of the form `crop=height:width:x:y`");
        }
        // TODO check that its a sane crop string
        Ok(Self(input.to_owned()))
    }
}

#[derive(Debug, ValueEnum, Clone, strum::Display)]
#[strum(serialize_all = "lowercase")]
pub enum VideoEncoder {
    Libx264,
    Libx265,
    Nvenc,
}

#[derive(Debug, ValueEnum, Clone, strum::Display)]
#[strum(serialize_all = "lowercase")]
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

#[derive(Debug, ValueEnum, Clone, strum::Display)]
#[strum(serialize_all = "lowercase")]
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
            let response = util::prompt("Please enter the title of the TV show:");
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
            match util::prompt("Enter the season index of the TV show:").parse::<usize>() {
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
            util::prompt("Enter the index of the first episode in this directory:").parse::<usize>()
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

pub fn validate_args(args: &Args) {
    if matches!(args.encoder, VideoEncoder::Nvenc) {
        if args.no_hwaccel {
            eprintln!("Hardware acceleration cannot be disabled when using nvenc");
            std::process::exit(1);
        }
        if args.tune.is_some() {
            eprintln!("Libx264 tunes cannot be used with nvenc.");
            std::process::exit(1);
        }
    }
}
