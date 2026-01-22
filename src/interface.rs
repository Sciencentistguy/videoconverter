use std::ops::Deref;
use std::path::PathBuf;
use std::str::FromStr;

use clap::Parser;
use clap::ValueEnum;
use clap::builder::ArgPredicate;
use regex::Regex;

const NNEDI_WEIGHTS_PATH: &str = "~/.ffmpeg/nnedi3_weights.bin";
const FFMPEG_BIN_PATH: &str = "ffmpeg";

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
    #[clap(long)]
    pub reencode_video: bool,

    /// Disable reencoding of video
    #[clap(long, conflicts_with_all = &["reencode_video", "force_deinterlace"])]
    pub copy_video: bool,

    /// Control the reencoding of audio
    #[clap(long, default_value = "pcm")]
    pub audio_reencoding: AudioReencodeType,

    /// Audio languages to keep, in the form of ISO 639-2 codes
    #[clap(
        long("audio-lang"),
        default_value("eng"),
        conflicts_with("all_streams"),
        default_values_if("anime", ArgPredicate::IsPresent, &["jpn", "eng"])
    )]
    pub audio_languages: Vec<String>,

    /// Subtitle languages to keep, in the form of ISO 639-2 codes
    #[clap(
        long("subtitle-lang"),
        default_value("eng"),
        conflicts_with("all_streams")
    )]
    pub subtitle_languages: Vec<String>,

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
    #[clap(
        short,
        long,
        default_value_if("print_commands", ArgPredicate::IsPresent, "true")
    )]
    pub simulate: bool,

    /// Print the ffmpeg command(s) that would be run. Implies `--simulate`
    #[clap(long)]
    pub print_commands: bool,

    /// Specify libx264 tune. Has no effect with Nvenc.
    #[clap(short, long, ignore_case = true, value_enum)]
    pub tune: Option<Libx264Tune>,

    /// The path to operate on
    #[clap(default_value = ".")]
    pub path: Vec<PathBuf>,

    /// The directory to generate `newfiles` in (or Season XX in TV mode)
    #[clap(long)]
    pub output_path: Option<PathBuf>,

    /// Enables renaming of files to TV show format
    #[clap(long, short = 'T')]
    pub tv_mode: bool,

    /// The season number to use in TV mode
    #[clap(long, required_if_eq("tv_mode", "true"))]
    pub season: Option<u32>,

    /// The episode number to use in TV mode
    #[clap(long, required_if_eq("tv_mode", "true"))]
    pub episode: Option<u32>,

    /// The title to use in TV mode
    #[clap(long, required_if_eq("tv_mode", "true"))]
    pub title: Option<String>,

    /// The path for the statefile
    #[clap(long = "statefile")]
    pub db_path: Option<PathBuf>,

    /// Spawn each ffmpeg command concurrently.
    #[clap(short, long, value_name = "CONCURRENCY_LIMIT")]
    pub parallel: Option<Option<usize>>,

    /// Moves the `default_audio_stream`th audio stream with the given language code to the front, and marks it as
    /// default.
    #[clap(
        long,
        value_name = "LANGUAGE",
        default_value_if("anime", ArgPredicate::IsPresent, "jpn")
    )]
    pub default_audio_language: Option<String>,

    /// Moves the `default_subtitle_stream`th subtitle stream with the given language code to the front, and marks it as
    /// default.
    #[clap(long, value_name = "LANGUAGE")]
    pub default_subtitle_language: Option<String>,

    /// Works in conjunction with `default_subtitle_language`.
    #[clap(long, value_name = "INDEX", default_value = "0")]
    pub default_audio_stream: usize,

    /// Works in conjunction with `default_subtitle_language`.
    #[clap(long, value_name = "INDEX", default_value = "0")]
    pub default_subtitle_stream: usize,

    /// Weights file for nnedi3 deinterlace filter
    #[clap(long, default_value = NNEDI_WEIGHTS_PATH)]
    pub nnedi_weights: String,

    /// Path to ffmpeg binary
    #[clap(long, default_value = FFMPEG_BIN_PATH)]
    pub ffmpeg_path: PathBuf,

    /// Overwrite output file, instead of erroring out.
    #[clap(long)]
    pub overwrite: bool,

    /// File extension to ignore. Can be specified multiple times.
    #[clap(long = "ignore", action = clap::ArgAction::Append, value_name = "EXTENSION")]
    pub ignored_extensions: Vec<String>,

    /// Do not prompt for confirmation before performing the conversion.
    #[clap(long, short, conflicts_with = "simulate")]
    pub yes: bool,

    /// The max depth to traverse when searching for files. Note: this does not effect the output
    /// file location
    #[clap(long, default_value = "1")]
    pub depth: usize,

    /// Continue processing items on error.
    #[clap(long = "continue")]
    pub continue_processing: bool,

    /// If passed, this will override the logic of which audio streams to keep
    #[clap(long)]
    pub override_audio: Vec<StreamRef>,

    /// If passed, this will override the logic of which subtitle streams to keep
    #[clap(long)]
    pub override_subs: Vec<StreamRef>,

    #[clap(long = "fflags")]
    pub input_fflags: Vec<String>,

    /// By default, videoconverter retains all attachment and data streams. This disables that
    /// behaviour
    #[clap(long, conflicts_with = "all_streams")]
    pub discard_attachments: bool,

    /// Normalize all stream titles to their language
    #[clap(long)]
    pub normalize_titles: bool,

    /// Implies '--default-audio-language jpn --audio-lang jpn --audio-lang eng'
    #[clap(long)]
    pub anime: bool,
}

impl Args {
    pub fn validate(&self) {
        if matches!(self.encoder, VideoEncoder::Nvenc) {
            if self.no_hwaccel {
                eprintln!("Hardware acceleration cannot be disabled when using nvenc");
                std::process::exit(1);
            }
            if self.tune.is_some() {
                eprintln!("Libx264 tunes cannot be used with nvenc.");
                std::process::exit(1);
            }
        }
    }
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
        let r = Regex::new(r"^crop=(\d+):(\d+):(\d+):(\d+)$").unwrap();
        let capts = r
            .captures(input)
            .ok_or("Error: Crop filter must be of the form `crop=height:width:x:y`")?;

        let height = capts.get(1).unwrap().as_str();
        let width = capts.get(2).unwrap().as_str();
        let x = capts.get(3).unwrap().as_str();
        let y = capts.get(4).unwrap().as_str();

        if x > width {
            return Err("Crop x value cannot be greater than width");
        }
        if y > height {
            return Err("Crop y value cannot be greater than height");
        }

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

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct StreamRef {
    file: usize,
    stream: usize,
}

impl StreamRef {
    pub fn new(file: usize, stream: usize) -> Self {
        Self { file, stream }
    }
}

impl FromStr for StreamRef {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (before, after) = s
            .split_once(':')
            .ok_or("StreamRef must be of the form <file>:<stream>")?;
        Ok(StreamRef {
            file: before.parse().map_err(|_| "Parse failed for file number")?,
            stream: after.parse().map_err(|_| "Parse failed for stream index")?,
        })
    }
}

#[allow(clippy::upper_case_acronyms)]
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum AudioReencodeType {
    /// Stream-copy all audio codecs
    None,
    /// Reencode PCM audio to FLAC, stream-copy all other codecs
    PCM,
    /// Reencode lossless audio to flac, stream-copy all other codecs
    Lossless,
    /// Reencode lossless audio to FLAC and lossy audio to AAC [Not Reccomended]
    All,
}
