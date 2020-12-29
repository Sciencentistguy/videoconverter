use crate::frontend::Stream;
use crate::frontend::StreamMappings;
use crate::interface::Opt;
use crate::interface::TVOptions;

use ffmpeg::codec;
use log::error;
use log::trace;
use simple_error::SimpleError;
use std::collections::HashMap;
use std::iter::Iterator;
use std::path::Path;
use std::process::Command;

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

fn get_encoder(codec: codec::Id) -> Result<&'static str, SimpleError> {
    use codec::Id;
    match codec {
        Id::AAC => Ok("libfdk_aac"),
        Id::FLAC => Ok("flac"),
        Id::H264 => Ok("libx264"),
        _ => {
            error!("Invalid codec '{:?}' passed to get_encoder.", codec);
            return Err(SimpleError::new("Invalid codec passed to get_encoder"));
        }
    }
}

pub fn generate_ffmpeg_command<P: AsRef<Path>>(
    input_path: P,
    output_path: P,
    mappings: &StreamMappings,
    codecs: &HashMap<usize, Option<codec::Id>>,
    //tv_options: &TVOptions,
    args: &Opt,
) -> Result<std::process::Command, Box<dyn std::error::Error>> {
    let mut command = Command::new("ffmpeg");

    let video_stream = &match &mappings.video[0] {
        Stream::Video(x) => Some(x),
        _ => None,
    }
    .unwrap();

    let reencoding_video = codecs.get(&0usize).unwrap().is_some();
    let reencoding_audio = itertools::any(mappings.audio.iter().map(|x| x.index()), |x| codecs.get(&x).unwrap().is_some());

    command.arg("-hide_banner");

    if !args.no_hwaccel {
        command.arg("-hwaccel");
        command.arg("auto");
    }

    command.arg("-i");
    command.arg(input_path.as_ref().as_os_str());
    command.args(&["-max_muxing_queue_size", "16384"]);

    let generate_codec_args = |command: &mut Command, stream_type: char, index_in: usize, index_out: usize| -> Result<(), SimpleError> {
        command.arg(format!("-c:{}:{}", stream_type, index_out));
        let codec = codecs.get(&index_in).expect("Codec not found in map");
        if codec.is_none() {
            command.arg("copy");
        } else {
            command.arg(get_encoder(codec.unwrap())?);
        }
        Ok(())
    };

    for (out_index, stream) in mappings.video.iter().enumerate() {
        generate_codec_args(&mut command, 'v', stream.index(), out_index)?;
    }

    if reencoding_video {
        trace!("Reencoding video");
        command.args(&["-profile:v", "high", "-rc-lookahead", "250", "-preset", "slow"]);
        if !args.no_hwaccel {
            command.args(&["-x264opts", "opencl"]);
        }

        if let Some(x) = args.tune.as_ref() {
            let s = x.to_string().to_lowercase();
            command.arg("-tune");
            command.arg(s);
        }

        let deinterlace = !args.no_deinterlace
            && (args.force_deinterlace
                || match video_stream.field_order {
                    crate::frontend::FieldOrder::Interlaced => true,
                    _ => false,
                });

        let crop = args.crop.is_some();

        if deinterlace || crop {
            command.arg("-filter:v");
        }

        if crop {
            let filter = args.crop.as_ref().unwrap();
            trace!("Cropping video with filter '{}'", filter);
            command.arg(format!("{}{}", filter, if deinterlace { "," } else { "" }));
        }

        if deinterlace {
            trace!("Deinterlacing video");
            command.arg("yadif");
        }
    }

    for (out_index, stream) in mappings.audio.iter().enumerate() {
        generate_codec_args(&mut command, 'a', stream.index(), out_index)?;
    }

    if reencoding_audio {
        trace!("Reencoding audio");
        // Flac ignores these, its just for libfdk_aac
        command.args(&["-cutoff", "18000", "-vbr", "5"]);
    }

    for (out_index, stream) in mappings.subtitle.iter().enumerate() {
        generate_codec_args(&mut command, 's', stream.index(), out_index)?;
    }

    for stream in mappings.iter() {
        command.arg("-map");
        command.arg(format!("0:{}", stream.index()));
    }

    command.arg(output_path.as_ref().as_os_str());

    return Ok(command);
}
