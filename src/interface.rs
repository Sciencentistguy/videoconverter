use crate::util;

use clap::arg_enum;
pub use structopt::StructOpt;


#[derive(StructOpt, Debug)]
#[structopt(setting(clap::AppSettings::ColoredHelp))]
#[structopt(name = "VideoConverter")]
pub struct Opt {
    /// Keep all streams, regardless of language metadata. [Not Yet Implemented]
    #[structopt(short, long)]
    pub all_streams: bool,

    /// Specify a CRF value to be passed to libx264
    #[structopt(long, default_value = "20")]
    pub crf: u8,

    /// Specify a crop filter. These are of the format 'crop=height:width:x:y'
    #[structopt(long)]
    pub crop: Option<String>,

    /// Force deinterlacing of video
    #[structopt(short = "-d", long)]
    pub force_deinterlace: bool,

    /// Disable automatic deinterlacing of video
    #[structopt(short = "-D", long, conflicts_with = "force_deinterlace")]
    pub no_deinterlace: bool,

    /// Force reencoding of video
    #[structopt(long)]
    pub force_reencode: bool,

    /// Use GPU accelerated encoding (nvenc). This produces HEVC. Requires an Nvidia 10-series gpu or later
    #[structopt(short, long, conflicts_with = "no_hwaccel")]
    pub gpu: bool,

    /// Disable hardware-accelerated decoding
    #[structopt(long)]
    pub no_hwaccel: bool,

    /// Do not actually perform the conversion
    #[structopt(short, long)]
    pub simulate: bool,

    /// Specify libx264 tune.
    #[structopt(short, long, possible_values = &Libx264Tune::variants(), case_insensitive=true, conflicts_with = "gpu")]
    pub tune: Option<Libx264Tune>,

    #[structopt(short, long)]
    pub verbose: bool,

    /// Write output to a log file [Not Yet Implemented]
    #[structopt(long)]
    pub log: bool,

    /// The path to operate on
    #[structopt(default_value = ".")]
    pub path: std::path::PathBuf,
}

arg_enum! {
    #[derive(Debug)]
    pub enum Libx264Tune {
        Film,
        Animation,
        Grain,
        Stillimage,
        Psnr,
        Ssim,
        Fastdecode,
        Zerolatency,
    }
}

pub struct TVOptions {
    pub enabled: bool,
    pub title: Option<String>,
    pub season: Option<usize>,
    pub episode: Option<usize>,
}

pub fn get_tv_options() -> std::io::Result<TVOptions> {
    let enabled = util::confirm("TV Show Mode", false)?;

    //let using = false; // for NYI save state feature

    let title = if enabled {
        Some(util::prompt("Please enter the title of the TV show")?)
    } else {
        None
    };

    let mut season = None;
    let mut episode = None;

    if enabled {
        loop {
            match util::prompt("Enter the season of the tv show")?.parse::<usize>() {
                Ok(x) => {
                    season = Some(x);
                    break;
                }
                Err(_) => {}
            }
        }

        loop {
            match util::prompt("Enter the episode of the tv show")?.parse::<usize>() {
                Ok(x) => {
                    episode = Some(x);
                    break;
                }
                Err(_) => {}
            }
        }
    }

    return Ok(TVOptions {enabled, title, season, episode});
}
