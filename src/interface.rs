use std::path::PathBuf;

use crate::state;
use crate::util;
use crate::ARGS;

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
    #[structopt(short = "-d", long, conflicts_with = "no-deinterlace")]
    pub force_deinterlace: bool,

    /// Disable automatic deinterlacing of video
    #[structopt(short = "-D", long, conflicts_with = "force-deinterlace")]
    pub no_deinterlace: bool,

    /// Force reencoding of video
    #[structopt(long = "force-reencode")]
    pub force_reencode_video: bool,

    /// Specify encoder to use.
    #[structopt(short = "m", long = "mode", default_value = "Libx264", case_insensitive=true, possible_values = &Encoder::variants())]
    pub encoder: Encoder,

    /// Disable hardware-accelerated decoding
    #[structopt(long)]
    pub no_hwaccel: bool,

    /// Do not actually perform the conversion
    #[structopt(short, long)]
    pub simulate: bool,

    /// Specify libx264 tune. Has no effect with Nvenc.
    #[structopt(short, long, possible_values = &Libx264Tune::variants(), case_insensitive=true)]
    pub tune: Option<Libx264Tune>,

    /// The path to operate on
    #[structopt(default_value = ".")]
    pub path: std::path::PathBuf,

    /// Enables renaming of files to TV show format
    #[structopt(long, short = "-T")]
    pub tv_mode: bool,

    /// The path for the statefile
    #[structopt(long, default_value = "/tmp/videoconverter.state")]
    pub statefile: PathBuf,
}

arg_enum! {
    #[derive(Debug)]
    pub enum Encoder {
        Libx264,
        Libx265,
        Nvenc,
    }
}

arg_enum! {
    #[derive(Debug)]
    pub enum Libx264Tune {
        Film,
        Animation,
        Grain,
        StillImage,
        PSNR,
        SSIM,
        FastDecode,
        ZeroLatency,
    }
}

#[derive(Debug)]
pub struct TVOptions {
    pub title: String,
    pub season: usize,
    pub episode: usize,
}

pub fn get_tv_options() -> Option<TVOptions> {
    let enabled =
        ARGS.tv_mode || util::confirm("TV Show Mode", false).expect("failed to get user input");
    if !enabled {
        return None;
    }

    let mut previous_state = state::read_state();
    let mut title = String::new();

    if let Some(ref mut previous_state) = previous_state {
        let use_old_value = util::confirm(
            &format!("Use previous title? ({})", previous_state.title),
            false,
        )
        .expect("failed to get user input");

        if use_old_value {
            title = std::mem::take(&mut previous_state.title);
        }
    }

    if title.is_empty() {
        title = loop {
            let response = util::prompt("Please enter the title of the TV show")
                .expect("failed to get user input");
            if !response.is_empty() {
                break response;
            }
        }
    }

    let mut season = None;

    if let Some(previous_state) = previous_state {
        print!("Use previous season? ({})", previous_state.season);
        let use_old_value = util::confirm("", false).expect("failed to get user input");

        if use_old_value {
            season = Some(previous_state.season);
        }
    }

    if season.is_none() {
        season = loop {
            if let Ok(x) = util::prompt("Enter the season index of the tv show")
                .expect("failed to get user input")
                .parse::<usize>()
            {
                break Some(x);
            }
        }
    }

    let episode = loop {
        if let Ok(x) = util::prompt("Enter the index of the first episode in this directory")
            .expect("failed to get user input")
            .parse::<usize>()
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
