use std::collections::HashMap;

use crate::interface::VideoEncoder;
use crate::ARGS;

pub use ffmpeg::codec;
pub use ffmpeg::codec::Context;
pub use ffmpeg::codec::Parameters;
pub use ffmpeg::format::context::Input;
pub use ffmpeg::media::Type;
use ffmpeg::ChannelLayout;
use tracing::*;

#[derive(Debug, Clone, Copy)]
pub enum FieldOrder {
    Progressive,
    Unknown,
    Interlaced,
}

#[derive(Debug, Clone)]
pub struct Video {
    pub index: usize,
    pub codec: codec::Id,
    pub field_order: FieldOrder,
}

#[derive(Debug, Clone)]
pub struct Audio {
    pub index: usize,
    pub codec: codec::Id,
    pub lang: Option<String>,
    pub channels: u16,
    pub channel_layout: ChannelLayout,
    pub profile: Option<ffmpeg::codec::Profile>,
    pub title: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Subtitle {
    pub index: usize,
    pub codec: codec::Id,
    pub lang: Option<String>,
    pub title: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Stream {
    Video(Video),
    Audio(Audio),
    Subtitle(Subtitle),
}

pub struct StreamMappings {
    pub video: Vec<Stream>,
    pub audio: Vec<Stream>,
    pub subtitle: Vec<Stream>,
}

impl Stream {
    pub fn index(&self) -> usize {
        match self {
            Stream::Video(x) => x.index,
            Stream::Audio(x) => x.index,
            Stream::Subtitle(x) => x.index,
        }
    }

    pub fn codec(&self) -> codec::Id {
        match self {
            Stream::Video(x) => x.codec,
            Stream::Audio(x) => x.codec,
            Stream::Subtitle(x) => x.codec,
        }
    }

    pub fn as_audio(&self) -> Option<&Audio> {
        if let Self::Audio(v) = self {
            Some(v)
        } else {
            None
        }
    }

    fn video(index: usize, codec_context: Context, codec_parameters: Parameters) -> Stream {
        let index = index;
        let codec = codec_parameters.id();

        let decoder = codec_context.decoder().video();
        let field_order = match unsafe { decoder.map(|x| (*x.as_ptr()).field_order) } {
            Ok(ffmpeg::ffi::AVFieldOrder::AV_FIELD_PROGRESSIVE) => FieldOrder::Progressive,
            Ok(ffmpeg::ffi::AVFieldOrder::AV_FIELD_TT) => FieldOrder::Interlaced,
            Ok(ffmpeg::ffi::AVFieldOrder::AV_FIELD_TB) => FieldOrder::Interlaced,
            Ok(ffmpeg::ffi::AVFieldOrder::AV_FIELD_BT) => FieldOrder::Interlaced,
            Ok(ffmpeg::ffi::AVFieldOrder::AV_FIELD_BB) => FieldOrder::Interlaced,
            Ok(ffmpeg::ffi::AVFieldOrder::AV_FIELD_UNKNOWN) => FieldOrder::Unknown,
            Err(x) => {
                error!(stream = %index, err = ?x, "Error getting field order");
                FieldOrder::Unknown
            }
        };

        Self::Video(Video {
            index,
            codec,
            field_order,
        })
    }

    fn audio(
        index: usize,
        codec_context: Context,
        codec_parameters: Parameters,
        tags: ffmpeg::DictionaryRef,
    ) -> Stream {
        let codec = codec_parameters.id();
        let lang = tags.get("language").map(|f| f.to_string());
        let decoder = codec_context.decoder().audio().unwrap();
        let channels = decoder.channels();
        let channel_layout = decoder.channel_layout();
        let profile = match decoder.profile() {
            codec::Profile::Unknown => None,
            x => Some(x),
        };
        let title = tags.get("title").map(|x| x.to_string());

        Self::Audio(Audio {
            index,
            codec,
            lang,
            channels,
            channel_layout,
            profile,
            title,
        })
    }

    fn subtitle(index: usize, codec_parameters: Parameters, tags: ffmpeg::DictionaryRef) -> Stream {
        let codec = codec_parameters.id();
        let lang = tags.get("language").map(|f| f.to_string());
        let title = tags.get("title").map(|x| x.to_string());

        Self::Subtitle(Subtitle {
            index,
            codec,
            lang,
            title,
        })
    }

    pub fn as_subtitle(&self) -> Option<&Subtitle> {
        if let Self::Subtitle(v) = self {
            Some(v)
        } else {
            None
        }
    }
}

impl StreamMappings {
    pub fn iter(&self) -> impl Iterator<Item = &Stream> {
        self.video
            .iter()
            .chain(self.audio.iter())
            .chain(self.subtitle.iter())
    }
}

pub fn parse_stream_metadata(file: Input) -> Vec<Stream> {
    file.streams()
        .filter_map(|stream| {
            let index = stream.index();
            let codec_parameters = stream.parameters();
            let codec_context =
                ffmpeg::codec::context::Context::from_parameters(codec_parameters.clone()).unwrap();
            // let codec_context = stream.codec();
            let tags = stream.metadata();

            match codec_context.medium() {
                Type::Video => Some(Stream::video(index, codec_context, codec_parameters)),
                Type::Audio => Some(Stream::audio(index, codec_context, codec_parameters, tags)),
                Type::Subtitle => Some(Stream::subtitle(index, codec_parameters, tags)),
                _ => None,
            }
        })
        .collect()
}

pub fn get_stream_mappings(parsed: &[Stream]) -> StreamMappings {
    let mut videos = Vec::new();
    let mut audios = Vec::new();
    let mut subtitles = Vec::new();

    for stream in parsed {
        match stream {
            Stream::Video(x) => {
                if x.codec != codec::Id::MJPEG {
                    videos.push(Stream::Video(x.clone()));
                }
            }

            Stream::Audio(x) => {
                if x.lang.as_deref() == Some(&ARGS.audio_language) || ARGS.all_streams {
                    audios.push(Stream::Audio(x.clone()));
                }
            }

            Stream::Subtitle(x) => {
                if x.lang.as_deref() == Some(&ARGS.subtitle_language) || ARGS.all_streams {
                    subtitles.push(Stream::Subtitle(x.clone()));
                }
            }
        }
    }

    match videos.len() {
        0 => {
            error!("No video streams found");
            std::process::exit(1);
        }
        1 => {}
        n => {
            warn!(%n, "File has multiple video streams. Only the first stream will be kept");
            videos.truncate(1);
        }
    }

    if audios.is_empty() {
        warn!("No english audio streams found. Retaining all audio streams");
        for stream in parsed {
            if let Stream::Audio(x) = stream {
                audios.push(Stream::Audio(x.clone()));
            }
        }
    }

    if subtitles.is_empty() {
        warn!("No english subtitle streams found. Retaining all subtitle streams");
        for stream in parsed {
            if let Stream::Subtitle(x) = stream {
                subtitles.push(Stream::Subtitle(x.clone()));
            }
        }
    }

    StreamMappings {
        video: videos,
        audio: audios,
        subtitle: subtitles,
    }
}

pub fn get_codec_mapping(stream_mappings: &StreamMappings) -> HashMap<usize, Option<codec::Id>> {
    use codec::Id::{AAC, DTS, DVD_SUBTITLE, FLAC, H264, HDMV_PGS_SUBTITLE, HEVC, SSA, TRUEHD};

    stream_mappings
        .iter()
        .map(|stream| {
            let index = stream.index();
            match stream {
                Stream::Video(video) if !ARGS.copy_video => match video.codec {
                    HEVC | H264 if !ARGS.force_reencode_video => (index, None),
                    _ => (
                        index,
                        Some(match ARGS.encoder {
                            VideoEncoder::Libx264 => H264,
                            VideoEncoder::Libx265 => HEVC,
                            VideoEncoder::Nvenc => HEVC,
                        }),
                    ),
                },
                Stream::Audio(audio) if ARGS.reencode_audio => match audio.codec {
                    FLAC | AAC => (index, None),
                    TRUEHD => (index, Some(FLAC)),
                    DTS if matches!(
                        audio.profile,
                        Some(codec::Profile::DTS(codec::profile::DTS::HD_MA))
                    ) =>
                    {
                        (index, Some(FLAC))
                    }
                    _ => (index, Some(AAC)),
                },
                Stream::Subtitle(subtitle) if ARGS.reencode_subs => match subtitle.codec {
                    HDMV_PGS_SUBTITLE | DVD_SUBTITLE => (index, None),
                    _ => (index, Some(SSA)),
                },
                _ => (index, None),
            }
        })
        .collect()
}
