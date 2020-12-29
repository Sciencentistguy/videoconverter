use crate::util;

use clap::arg_enum;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(setting(clap::AppSettings::ColoredHelp))]
#[structopt(name = "VideoConverter")]
pub struct Opt {
    /// Keep all streams, regardless of language metadata. [Not Yet Implemented]
    #[structopt(short, long)]
    pub all_streams: bool,

    /// Specify a CRF value to be passed to libx264 [Not Yet Implemented]
    #[structopt(long, default_value = "20")]
    pub crf: u8,

    /// Specify a crop filter. These are of the format 'crop=height:width:x:y' [Not Yet Implemented]
    #[structopt(long)]
    pub crop: Option<String>,

    /// Force deinterlacing of video [Not Yet Implemented]
    #[structopt(short, long)]
    pub deinterlace: bool,

    /// Disable automatic deinterlacing of video [Not Yet Implemented]
    #[structopt(short = "-D", long)]
    pub no_deinterlace: bool,

    /// Force reencoding of video [Not Yet Implemented]
    #[structopt(long)]
    pub force_reencode: bool,

    /// Use GPU accelerated encoding (nvenc). This produces HEVC. Requires an Nvidia 10-series gpu
    /// or later [Not Yet Implemented]
    #[structopt(short, long)]
    pub gpu: bool,

    /// Disable hardware-accelerated decoding [Not Yet Implemented]
    #[structopt(long)]
    pub no_hwaccel: bool,

    /// Do not actually perform the conversion [Not Yet Implemented]
    #[structopt(short, long)]
    pub simulate: bool,

    /// Specify libx264 tune. Incompatible with --gpu [Not Yet Implemented]
    #[structopt(short, long, possible_values = &Libx264Tune::variants(), case_insensitive=true)]
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

pub fn get_tv_options() -> std::io::Result<(bool, Option<String>, Option<usize>, Option<usize>)> {
    let tv_mode = util::confirm("TV Show Mode", false)?;

    //let using = false; // for NYI save state feature

    let tv_show_title = if tv_mode {
        Some(util::prompt("Please enter the title of the TV show")?)
    } else {
        None
    };

    let mut tv_show_season = None;
    let mut tv_show_episode = None;

    if tv_mode {
        loop {
            match util::prompt("Enter the season of the tv show")?.parse::<usize>() {
                Ok(x) => {
                    tv_show_season = Some(x);
                    break;
                }
                Err(_) => {}
            }
        }

        loop {
            match util::prompt("Enter the episode of the tv show")?.parse::<usize>() {
                Ok(x) => {
                    tv_show_episode = Some(x);
                    break;
                }
                Err(_) => {}
            }
        }
    }

    return Ok((tv_mode, tv_show_title, tv_show_season, tv_show_episode));
}
