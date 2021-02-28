use crate::util;

use clap::arg_enum;
pub use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(setting(clap::AppSettings::ColoredHelp))]
#[structopt(name = "videoconverter")]
pub struct Opt {
    /// Keep all streams, regardless of language metadata.
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

pub fn get_tv_options() -> Result<TVOptions, Box<dyn std::error::Error>> {
    let enabled = util::confirm("TV Show Mode", false)?;
    if !enabled {
        return Ok(TVOptions {
            enabled,
            title: None,
            season: None,
            episode: None,
        });
    }

    let previous = match util::read_state() {
        Ok(x) => Some(x),
        Err(_) => None,
    };

    let mut using_previous = previous.is_some();
    let mut title: Option<String> = None;

    if using_previous {
        print!(
            "Use previous title? ({})",
            previous.as_ref().unwrap().title.as_ref().unwrap()
        );
        let b = util::confirm("", false)?;
        if b {
            title = previous.as_ref().unwrap().title.clone();
        } else {
            using_previous = false;
        }
    }
    if !using_previous {
        loop {
            title = Some(util::prompt("Please enter the title of the TV show")?);
            if title.as_ref().unwrap().is_empty() {
                continue;
            }
            break;
        }
    }

    let mut season = None;

    if using_previous {
        print!(
            "Use previous season? ({})",
            previous.as_ref().and_then(|x| x.season.as_ref()).unwrap()
        );
        let b = util::confirm("", false)?;
        if b {
            season = previous.as_ref().unwrap().season;
        } else {
            using_previous = false;
        }
    }

    if !using_previous {
        loop {
            if let Ok(x) = util::prompt("Enter the season index of the tv show")?.parse::<usize>() {
                season = Some(x);
                break;
            }
        }
    }

    let episode = loop {
        if let Ok(x) =
            util::prompt("Enter the index of the first episode in this directory")?.parse::<usize>()
        {
            break Some(x);
        }
    };

    Ok(TVOptions {
        enabled,
        title,
        season,
        episode,
    })
}
