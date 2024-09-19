use std::collections::HashMap;

use crate::interface::StreamRef;
use crate::interface::VideoEncoder;
use crate::ARGS;

pub use ffmpeg::codec;
pub use ffmpeg::codec::Context;
pub use ffmpeg::codec::Parameters;
pub use ffmpeg::format::context::Input;
pub use ffmpeg::media::Type;
use ffmpeg::ChannelLayout;
use ffmpeg_sys_the_third::AVChannelLayout;
use tracing::*;

#[derive(Debug, Clone, Copy)]
pub enum FieldOrder {
    Progressive,
    Unknown,
    Interlaced,
}

#[derive(Debug, Clone)]
pub struct Video {
    pub file: usize,
    pub index: usize,
    pub codec: codec::Id,
    pub field_order: FieldOrder,
}

#[derive(Debug, Clone)]
pub struct Audio {
    pub file: usize,
    pub index: usize,
    pub codec: codec::Id,
    pub lang: Option<String>,
    pub channels: u32,
    pub channel_layout: AVChannelLayout,
    pub profile: Option<ffmpeg::codec::Profile>,
    pub title: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Subtitle {
    pub file: usize,
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
    pub fn file(&self) -> usize {
        match self {
            Stream::Video(x) => x.file,
            Stream::Audio(x) => x.file,
            Stream::Subtitle(x) => x.file,
        }
    }

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

    fn video(
        file: usize,
        index: usize,
        codec_context: Context,
        codec_parameters: Parameters,
    ) -> Self {
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
            Ok(_) => FieldOrder::Unknown,

            Err(x) => {
                error!(stream = %index, err = ?x, "Error getting field order");
                FieldOrder::Unknown
            }
        };

        Self::Video(Video {
            file,
            index,
            codec,
            field_order,
        })
    }

    fn audio(
        file: usize,
        index: usize,
        codec_context: Context,
        codec_parameters: Parameters,
        tags: ffmpeg::DictionaryRef,
    ) -> Stream {
        let codec = codec_parameters.id();
        let lang = tags.get("language").map(|f| f.to_string());
        let decoder = codec_context.decoder().audio().unwrap();
        let channel_layout = decoder.ch_layout().to_owned();
        let channels = channel_layout.channels();
        let profile = match decoder.profile() {
            codec::Profile::Unknown => None,
            x => Some(x),
        };
        let title = tags.get("title").map(|x| x.to_string());

        Self::Audio(Audio {
            file,
            index,
            codec,
            lang,
            channels,
            channel_layout: channel_layout.into_owned(),
            profile,
            title,
        })
    }

    fn subtitle(
        file: usize,
        index: usize,
        codec_parameters: Parameters,
        tags: ffmpeg::DictionaryRef,
    ) -> Stream {
        let codec = codec_parameters.id();
        let lang = tags.get("language").map(|f| f.to_string());
        let title = tags.get("title").map(|x| x.to_string());

        Self::Subtitle(Subtitle {
            file,
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

pub fn parse_stream_metadata(file: Input, fileno: usize) -> Vec<Stream> {
    file.streams()
        .filter_map(|stream| {
            let index = stream.index();
            let codec_parameters = stream.parameters();
            let codec_context =
                ffmpeg::codec::context::Context::from_parameters(codec_parameters.clone()).unwrap();
            // let codec_context = stream.codec();
            let tags = stream.metadata();

            match codec_context.medium() {
                Type::Video => Some(Stream::video(
                    fileno,
                    index,
                    codec_context,
                    codec_parameters,
                )),
                Type::Audio => Some(Stream::audio(
                    fileno,
                    index,
                    codec_context,
                    codec_parameters,
                    tags,
                )),
                Type::Subtitle => Some(Stream::subtitle(fileno, index, codec_parameters, tags)),
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
                if !ARGS.override_audio.is_empty() {
                    if ARGS
                        .override_audio
                        .contains(&StreamRef::new(x.file, x.index))
                    {
                        audios.push(Stream::Audio(x.clone()));
                    }
                } else if x.lang.as_deref() == Some(&ARGS.audio_language) || ARGS.all_streams {
                    audios.push(Stream::Audio(x.clone()));
                }
            }

            Stream::Subtitle(x) => {
                if !ARGS.override_subs.is_empty() {
                    if ARGS
                        .override_subs
                        .contains(&StreamRef::new(x.file, x.index))
                    {
                        subtitles.push(Stream::Subtitle(x.clone()));
                    }
                } else if x.lang.as_deref() == Some(&ARGS.subtitle_language) || ARGS.all_streams {
                    subtitles.push(Stream::Subtitle(x.clone()));
                }
            }
        }
    }

    match videos.len() {
        0..=1 => {}
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

#[rustfmt::skip]
fn is_pcm(x: codec::Id) -> bool {
    use codec::Id::*;

    // List pulled from the definition of codec::Id
    matches!(
        x,
        PCM_S16LE | PCM_S16BE | PCM_U16LE
        | PCM_U16BE | PCM_S8 | PCM_U8
        | PCM_MULAW | PCM_ALAW | PCM_S32LE
        | PCM_S32BE | PCM_U32LE | PCM_U32BE
        | PCM_S24LE | PCM_S24BE | PCM_U24LE
        | PCM_U24BE | PCM_S24DAUD | PCM_ZORK
        | PCM_S16LE_PLANAR | PCM_DVD | PCM_F32BE
        | PCM_F32LE | PCM_F64BE | PCM_F64LE
        | PCM_BLURAY | PCM_LXF | S302M
        | PCM_S8_PLANAR | PCM_S24LE_PLANAR | PCM_S32LE_PLANAR
        | PCM_S16BE_PLANAR | PCM_S64LE | PCM_S64BE
    )
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
                    c if is_pcm(c) => (index, Some(FLAC)),
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
