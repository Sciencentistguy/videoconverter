use std::collections::HashMap;
use std::iter::Iterator;
use std::path::Path;
use std::path::PathBuf;

use crate::input::FieldOrder;
use crate::input::Stream;
use crate::input::StreamMappings;
use crate::interface::VideoEncoder;
use crate::tv::TVOptions;
use crate::ARGS;

use ffmpeg::codec;
use itertools::Itertools;
use tokio::process::Command;
use tracing::*;

const FFMPEG_BIN_PATH: &str = "ffmpeg";

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
            .expect("input_filepath should have a filename")
            .to_str()
            .unwrap();
        let input_ext = path
            .extension()
            .expect("input_filepath should have an extension")
            .to_str()
            .unwrap();
        input_filename.replace(input_ext, "mkv")
    }
}

trait GetEncoderExt {
    fn get_encoder(&self) -> &'static str;
}

impl GetEncoderExt for codec::Id {
    fn get_encoder(&self) -> &'static str {
        use codec::Id;
        match self {
            Id::AAC => "libfdk_aac",
            Id::FLAC => "flac",
            Id::H264 => "libx264",
            Id::HEVC => match ARGS.encoder {
                VideoEncoder::Libx264 => {
                    error!("Internal error: HEVC is not supported with libx264");
                    unreachable!();
                }
                VideoEncoder::Libx265 => "libx265",
                VideoEncoder::Nvenc => "hevc_nvenc",
            },
            Id::SSA => "ass",
            _ => {
                error!(
                    codec=?self,
                    "Internal error: Unexpected output codec passed to get_encoder."
                );
                unreachable!();
            }
        }
    }
}

pub fn generate_ffmpeg_command<P: AsRef<Path>>(
    input_path: P,
    associated_subs: &[PathBuf],
    output_path: P,
    mut mappings: StreamMappings,
    target_codecs: HashMap<usize, Option<codec::Id>>,
) -> Result<Command, CommandError> {
    let mut command = Command::new(FFMPEG_BIN_PATH);
    command.arg("-hide_banner"); // Remove gpl banner

    if !ARGS.simulate && output_path.as_ref().exists() {
        if ARGS.overwrite {
            warn!(file = ?output_path.as_ref().to_string_lossy(),
                "Output file already exists. Overwriting"
            );
        } else {
            error!(file = ?output_path.as_ref().to_string_lossy(),
                "Output file already exists."
            );
            return Err(CommandError::FileExists);
        }
    }

    let video_stream = match mappings.video.get(0) {
        Some(Stream::Video(x)) => x,
        _ => {
            error!("Input file does not contain a video stream.");
            std::process::exit(1);
        }
    };

    // Reencode video if:
    // - The video codec is not the same as the target codec
    // - `--deinterlace` is passed
    // - `--force-reencode` is passed
    let reencoding_video = target_codecs[&video_stream.index].is_some()
        || ARGS.force_deinterlace
        || ARGS.force_reencode_video;

    let reencoding_audio = mappings
        .audio
        .iter()
        .map(|x| x.index())
        .any(|x| target_codecs[&x].is_some());

    if reencoding_video && !ARGS.no_hwaccel {
        use codec::Id;
        match video_stream.codec {
            Id::H264 => {
                command.arg("-c:v");
                command.arg("h264_cuvid");
            }
            Id::HEVC => {
                command.arg("-c:v");
                command.arg("hevc_cuvid");
            }
            Id::MJPEG => {
                command.arg("-c:v");
                command.arg("mjpeg_cuvid");
            }
            Id::MPEG1VIDEO => {
                command.arg("-c:v");
                command.arg("mpeg1_cuvid");
            }
            Id::MPEG2VIDEO => {
                command.arg("-c:v");
                command.arg("mpeg2_cuvid");
            }
            Id::MPEG4 => {
                command.arg("-c:v");
                command.arg("mpeg4_cuvid");
            }
            Id::VC1 => {
                command.arg("-c:v");
                command.arg("vc1_cuvid");
            }
            Id::VP8 => {
                command.arg("-c:v");
                command.arg("vp8_cuvid");
            }
            Id::VP9 => {
                command.arg("-c:v");
                command.arg("vp9_cuvid");
            }

            _ => {
                warn!("No hardware acceleration available for video stream. Using generic mode.");
                command.arg("-hwaccel");
                command.arg("auto");
            }
        }
    }

    if !ARGS.input_fflags.is_empty() {
        command.arg("-fflags");
        command.arg(ARGS.input_fflags.join(""));
    }

    command.arg("-i");
    command.arg(input_path.as_ref().as_os_str());

    for path in associated_subs {
        command.arg("-i");
        command.arg(path.as_os_str());
    }

    // With large files this is needed to avoid an ffmpeg crash
    command.args(["-max_muxing_queue_size", "16384"]);

    let generate_codec_args =
        |command: &mut Command, stream_type: char, index_in: usize, index_out: usize| {
            let codec = target_codecs[&index_in];
            command.arg(format!("-c:{}:{}", stream_type, index_out));
            if let Some(&codec) = codec.as_ref() {
                command.arg(codec.get_encoder());
            } else if mappings.video.iter().map(|x| x.index()).contains(&index_in)
                && ARGS.force_reencode_video
            {
                let encoder = match ARGS.encoder {
                    VideoEncoder::Libx264 => "libx264",
                    VideoEncoder::Libx265 => "libx265",
                    VideoEncoder::Nvenc => "hevc_nvenc",
                };
                command.arg(encoder);
            } else {
                command.arg("copy");
            }
        };

    for (out_index, stream) in mappings.video.iter().enumerate() {
        generate_codec_args(&mut command, 'v', stream.index(), out_index);
    }

    const NVENC_FLAGS: &[&str] = &["-profile:v", "main", "-b:v", "0", "-rc-lookahead", "32"];

    const LIBX264_FLAGS: &[&str] = &["-profile:v", "high", "-rc-lookahead", "250"];

    const LIBX265_FLAGS: &[&str] = &["-profile:v", "main10", "-x265-params", "rc-lookahead=250"];

    const LIBFDK_AAC_FLAGS: &[&str] = &["-cutoff", "18000", "-vbr", "5"];

    if reencoding_video {
        // Insert the encoder flags for the video stream
        match ARGS.encoder {
            VideoEncoder::Libx264 => {
                command.arg("-crf");
                command.arg(ARGS.crf.to_string());
                command.args(LIBX264_FLAGS);

                if let Some(ref x) = ARGS.tune {
                    let s = x.to_string().to_lowercase();
                    command.arg("-tune");
                    command.arg(s);
                }
            }
            VideoEncoder::Libx265 => {
                command.arg("-crf");
                command.arg(ARGS.crf.to_string());
                command.args(LIBX265_FLAGS);
                if ARGS.tune.is_some() {
                    warn!("Tune is not supported for libx265");
                }
            }
            VideoEncoder::Nvenc => {
                command.args(["-rc", "constqp", "-qp"]);
                command.arg(ARGS.crf.to_string());
                command.args(NVENC_FLAGS);
            }
        }

        // Apply video encoder preset.
        command.arg("-preset");
        command.arg(ARGS.preset.to_string());

        // Whether to deinterlace the video.
        let deinterlace = matches!(video_stream.field_order, FieldOrder::Interlaced)
            && ARGS.no_deinterlace
            || ARGS.force_deinterlace;

        // Using an array instead of 2 variables so Iterator::join() can be used.
        // let mut filter_args = [None; 2];
        // let [crop_filter, deinterlace_filter] = &mut filter_args;

        // If a crop filter is set, use it.
        let crop_filter = ARGS.crop.as_ref().map(|x| x.to_string());

        let deinterlace_filter = if deinterlace {
            const NNEDI_FILTER: &str = "idet,fieldmatch=mode=pc_n_ub:combmatch=full:combpel=70,nnedi=deint=interlaced:pscrn=none:threads=32:weights=";

            trace!("Deinterlacing video");
            Some(format!("{}{}", NNEDI_FILTER, ARGS.nnedi_weights))
        } else {
            None
        };

        let it = std::iter::once(crop_filter).chain(std::iter::once(deinterlace_filter));
        if it.clone().any(|x| x.is_some()) {
            command.arg("-filter:v");
            command.arg(it.flatten().join(","));
        }
    }

    for (out_index, stream) in mappings.audio.iter().enumerate() {
        generate_codec_args(&mut command, 'a', stream.index(), out_index);
    }

    if reencoding_audio && target_codecs.values().contains(&Some(codec::Id::AAC)) {
        // Apply libfdk_aac flags if it is being used.
        command.args(LIBFDK_AAC_FLAGS);
    }

    for (out_index, stream) in mappings.subtitle.iter().enumerate() {
        generate_codec_args(&mut command, 's', stream.index(), out_index);
    }

    if let Some(lang) = ARGS.default_audio_language.as_deref() {
        match mappings
            .audio
            .iter()
            .enumerate()
            .filter(|(_, stream)| stream.as_audio().and_then(|x| x.lang.as_deref()) == Some(lang))
            .map(|(idx, _)| idx)
            .nth(ARGS.default_audio_stream)
        {
            None => {
                error!(
                    filename = ?input_path.as_ref(),
                    "Stream with language {lang} could not be found. Has it been discarded?"
                );
            }
            Some(target_stream_idx) => {
                if mappings.audio.len() > 1 {
                    // Move the chosen stream to the front
                    mappings.audio.swap(0, target_stream_idx);
                }

                // Set the default disposition for all audio streams to 0 (not default)
                for stream_idx in 1..mappings.audio.len() {
                    command.arg(format!("-disposition:a:{}", stream_idx));
                    command.arg("0");
                }

                // Then mark the first stream as default
                command.arg("-disposition:a:0");
                command.arg("default");
            }
        }
    }

    if let Some(lang) = ARGS.default_subtitle_language.as_deref() {
        match mappings
            .subtitle
            .iter()
            .enumerate()
            .filter(|(_, stream)| {
                stream.as_subtitle().and_then(|x| x.lang.as_deref()) == Some(lang)
            })
            .map(|(idx, _)| idx)
            .nth(ARGS.default_subtitle_stream)
        {
            None => {
                error!(
                    filename = ?input_path.as_ref(),
                    "Stream with language {lang} could not be found. Has it been discarded?"
                );
            }
            Some(target_stream_idx) => {
                if mappings.subtitle.len() > 1 {
                    // Move the chosen stream to the front
                    mappings.subtitle.swap(0, target_stream_idx);
                }

                // Set the default disposition for all subtitle streams to 0 (not default)
                for stream_idx in 1..mappings.subtitle.len() {
                    command.arg(format!("-disposition:s:{}", stream_idx));
                    command.arg("0");
                }

                // Then mark the first stream as default
                command.arg("-disposition:s:0");
                command.arg("default");
            }
        }
    }

    // Map each stream from the input file
    for stream in mappings.iter() {
        command.arg("-map");
        command.arg(format!("{}:{}", stream.file(), stream.index()));
    }

    for i in 0..associated_subs.len() + 1 {
        if ARGS.all_streams {
            // Retain attachments and data
            command.args(["-map", &format!("{}:d:?", i)]);
            command.args(["-map", &format!("{}:t:?", i)]);
        }
    }
    command.arg(output_path.as_ref().as_os_str());

    Ok(command)
}

#[derive(Debug)]
pub enum CommandError {
    FileExists,
}
