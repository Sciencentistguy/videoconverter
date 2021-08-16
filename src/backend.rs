use std::collections::HashMap;
use std::iter::Iterator;
use std::path::Path;
use std::process::Command;

use crate::frontend::Stream;
use crate::frontend::StreamMappings;
use crate::interface::Encoder;
use crate::interface::TVOptions;
use crate::ARGS;

use ffmpeg::codec;
use itertools::Itertools;
use log::*;

pub fn generate_output_filename<P: AsRef<Path>>(path: P, tv_options: &Option<TVOptions>) -> String {
    let path = path.as_ref();
    if let Some(tv_options) = tv_options {
        format!(
            "{} - s{:02}e{:02}.mkv",
            tv_options.title, tv_options.season, tv_options.episode
        )
    } else {
        let input_filename = path
            .file_name()
            .expect("Input filename is None")
            .to_str()
            .unwrap();
        let input_ext = path
            .extension()
            .expect("Input ext is None")
            .to_str()
            .unwrap();
        input_filename.replace(input_ext, "mkv")
    }
}

fn get_encoder(codec: codec::Id) -> &'static str {
    use codec::Id;
    match codec {
        Id::AAC => "libfdk_aac",
        Id::FLAC => "flac",
        Id::H264 => "libx264",
        Id::HEVC => match ARGS.encoder {
            Encoder::Libx264 => unreachable!(),
            Encoder::Libx265 => "libx265",
            Encoder::Nvenc => "hevc_nvenc",
        },
        Id::SSA => "ass",
        _ => panic!("Invalid output codec '{:?}' passed to get_encoder.", codec),
    }
}

pub fn generate_ffmpeg_command<P: AsRef<Path>>(
    input_path: P,
    output_path: P,
    mappings: StreamMappings,
    target_codecs: HashMap<usize, Option<codec::Id>>,
) -> std::process::Command {
    let mut command = Command::new("ffmpeg");
    command.arg("-hide_banner"); // Remove gpl banner

    if output_path.as_ref().exists() {
        error!(
            "Output file {} already exists. Exiting",
            output_path.as_ref().to_string_lossy()
        );
        std::process::exit(1);
    }

    let video_stream = match mappings.video.get(0) {
        Some(Stream::Video(x)) => x,
        _ => panic!("File does not have a video stream."),
    };

    let reencoding_video =
        target_codecs[&video_stream.index].is_some() || ARGS.force_reencode_video;
    let reencoding_audio = mappings
        .audio
        .iter()
        .map(|x| x.index())
        .any(|x| target_codecs[&x].is_some());

    if !ARGS.no_hwaccel {
        command.arg("-hwaccel");
        command.arg("auto");
    }

    command.arg("-i");
    command.arg(input_path.as_ref().as_os_str());

    // With large files this is needed to avoid an ffmpeg crash
    command.args(&["-max_muxing_queue_size", "16384"]);

    let generate_codec_args =
        |command: &mut Command, stream_type: char, index_in: usize, index_out: usize| {
            let codec = target_codecs[&index_in];
            command.arg(format!("-c:{}:{}", stream_type, index_out));
            if let Some(&codec) = codec.as_ref() {
                command.arg(get_encoder(codec));
            } else if ARGS.force_reencode_video {
                let encoder = match ARGS.encoder {
                    Encoder::Libx264 => "libx264",
                    Encoder::Libx265 => "libx265",
                    Encoder::Nvenc => "hevc_nvenc",
                };
                command.arg(encoder);
            } else {
                command.arg("copy");
            }
        };

    for (out_index, stream) in mappings.video.iter().enumerate() {
        generate_codec_args(&mut command, 'v', stream.index(), out_index);
    }

    const NVENC_FLAGS: &[&str] = &[
        "-preset",
        "slow",
        "-profile:v",
        "main",
        "-b:v",
        "0",
        "-rc-lookahead",
        "32",
    ];

    const LIBX264_FLAGS: &[&str] = &[
        "-profile:v",
        "high",
        "-rc-lookahead",
        "250",
        "-preset",
        "slow",
    ];

    const LIBX265_FLAGS: &[&str] = &[
        "-profile:v",
        "main10",
        "-preset",
        "slow",
        "-x265-params",
        "rc-lookahead=250",
    ];

    const LIBFDK_AAC_FLAGS: &[&str] = &["-cutoff", "18000", "-vbr", "5"];

    if reencoding_video {
        trace!("Reencoding video");
        match ARGS.encoder {
            Encoder::Libx264 => {
                command.arg("-crf");
                command.arg(ARGS.crf.to_string());
                command.args(LIBX264_FLAGS);
                if !ARGS.no_hwaccel {
                    command.args(&["-x264opts", "opencl"]);
                }

                if let Some(ref x) = ARGS.tune {
                    let s = x.to_string().to_lowercase();
                    command.arg("-tune");
                    command.arg(s);
                }
            }
            Encoder::Libx265 => {
                command.arg("-crf");
                command.arg(ARGS.crf.to_string());
                command.args(LIBX265_FLAGS);
                if let Some(ref x) = ARGS.tune {
                    warn!("Using libx264 tune with libx265");
                    let s = x.to_string().to_lowercase();
                    command.arg("-tune");
                    command.arg(s);
                }
            }
            Encoder::Nvenc => {
                command.args(&["-rc", "constqp", "-qp"]);
                command.arg(ARGS.crf.to_string());
                command.args(NVENC_FLAGS);
            }
        }

        let mut filter_args: [Option<&str>; 2] = [None; 2];

        let should_deinterlace = matches!(
            video_stream.field_order,
            crate::frontend::FieldOrder::Interlaced
        );
        let deinterlace = should_deinterlace && ARGS.no_deinterlace || ARGS.force_deinterlace;

        filter_args[0] = if let Some(ref filter) = ARGS.crop {
            trace!("Cropping video with filter '{}'", filter);
            ARGS.crop.as_deref()
        } else {
            None
        };

        filter_args[1] = if deinterlace {
            trace!("Deinterlacing video");
            if matches!(ARGS.encoder, Encoder::Nvenc) {
                Some("hwupload_cuda,yadif_cuda")
            } else {
                Some("yadif")
            }
        } else {
            None
        };

        if filter_args.iter().any(Option::is_some) {
            command.arg("-filter:v");
            command.arg(filter_args.iter().flatten().join(","));
        }
    }

    for (out_index, stream) in mappings.audio.iter().enumerate() {
        generate_codec_args(&mut command, 'a', stream.index(), out_index);
    }

    if reencoding_audio {
        trace!("Reencoding audio");
        if target_codecs.values().contains(&Some(codec::Id::AAC)) {
            command.args(LIBFDK_AAC_FLAGS);
        }
    }

    for (out_index, stream) in mappings.subtitle.iter().enumerate() {
        generate_codec_args(&mut command, 's', stream.index(), out_index);
    }

    for stream in mappings.iter() {
        command.arg("-map");
        command.arg(format!("0:{}", stream.index()));
    }

    command.arg(output_path.as_ref().as_os_str());

    command
}
