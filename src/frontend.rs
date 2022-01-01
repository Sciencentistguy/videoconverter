use std::collections::HashMap;
use std::mem;

use crate::interface::VideoEncoder;
use crate::ARGS;

pub use ffmpeg::codec;
pub use ffmpeg::codec::Context;
pub use ffmpeg::codec::Parameters;
pub use ffmpeg::format::context::Input;
pub use ffmpeg::media::Type;
use tracing::*;

#[derive(Debug)]
pub enum FieldOrder {
    Progressive,
    Unknown,
    Interlaced,
}

#[derive(Debug)]
pub struct Video {
    pub index: usize,
    pub codec: codec::Id,
    pub field_order: FieldOrder,
}

#[derive(Debug)]
pub struct Audio {
    pub index: usize,
    pub codec: codec::Id,
    pub lang: Option<String>,
    pub profile: Option<ffmpeg::codec::Profile>,
}

#[derive(Debug)]
pub struct Subtitle {
    pub index: usize,
    pub codec: codec::Id,
    pub lang: Option<String>,
}

#[derive(Debug)]
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
}

impl StreamMappings {
    pub fn iter(&self) -> impl Iterator<Item = &Stream> {
        self.video
            .iter()
            .chain(self.audio.iter())
            .chain(self.subtitle.iter())
    }
}

impl Video {
    pub fn new(index: usize, codec_context: Context, codec_par: Parameters) -> Video {
        let codec = codec_par.id();

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

        Video {
            index,
            codec,
            field_order,
        }
    }
}

impl Default for Video {
    fn default() -> Self {
        Video {
            index: 0,
            codec: codec::Id::None,
            field_order: FieldOrder::Unknown,
        }
    }
}

impl Audio {
    pub fn new(
        index: usize,
        codec_context: Context,
        codec_par: Parameters,
        metadata: ffmpeg::util::dictionary::Ref<'_>,
    ) -> Audio {
        let codec = codec_par.id();
        let lang = metadata.get("language").map(|f| f.to_string());
        let decoder = codec_context.decoder().audio();
        let profile = match decoder.map(|x| x.profile()) {
            Ok(codec::Profile::Unknown) => None,
            Ok(x) => Some(x),
            Err(_) => None,
        };

        Audio {
            index,
            codec,
            lang,
            profile,
        }
    }
}

impl Default for Audio {
    fn default() -> Self {
        Audio {
            index: 0,
            codec: codec::Id::None,
            lang: None,
            profile: None,
        }
    }
}

impl Subtitle {
    pub fn new(
        index: usize,
        codec_par: Parameters,
        metadata: ffmpeg::util::dictionary::Ref<'_>,
    ) -> Subtitle {
        let codec = codec_par.id();
        let lang = metadata.get("language").map(|f| f.to_string());

        Subtitle { index, codec, lang }
    }
}

impl Default for Subtitle {
    fn default() -> Self {
        Subtitle {
            index: 0,
            codec: codec::Id::None,
            lang: None,
        }
    }
}

pub fn parse_stream_metadata(file: Input) -> Vec<Stream> {
    let mut out: Vec<Stream> = Vec::new();
    for stream in file.streams() {
        let index = stream.index();
        let codec_context = stream.codec();
        let codec_parameters = stream.parameters();
        let tags = stream.metadata();
        //let explode = codec.codec().unwrap();
        match codec_context.medium() {
            Type::Video => out.push(Stream::Video(Video::new(
                index,
                codec_context,
                codec_parameters,
            ))),

            Type::Audio => out.push(Stream::Audio(Audio::new(
                index,
                codec_context,
                codec_parameters,
                tags,
            ))),

            Type::Subtitle => out.push(Stream::Subtitle(Subtitle::new(
                index,
                codec_parameters,
                tags,
            ))),
            _ => {}
        };
    }
    out
}

pub fn get_stream_mappings(mut parsed: Vec<Stream>) -> StreamMappings {
    let mut videos: Vec<Stream> = Vec::new();
    let mut audios: Vec<Stream> = Vec::new();
    let mut subtitles: Vec<Stream> = Vec::new();

    for stream in parsed.iter_mut() {
        match stream {
            Stream::Video(ref mut x) => {
                if x.codec != codec::Id::MJPEG {
                    videos.push(Stream::Video(mem::take(x)));
                }
            }
            Stream::Audio(ref mut x) => {
                if x.lang == Some("eng".to_string()) || ARGS.all_streams {
                    audios.push(Stream::Audio(mem::take(x)));
                }
            }
            Stream::Subtitle(ref mut x) => {
                if x.lang == Some("eng".to_string()) || ARGS.all_streams {
                    subtitles.push(Stream::Subtitle(mem::take(x)));
                }
            }
        }
    }

    if videos.len() != 1 {
        let num_vids = videos.len();
        warn!(n = ?num_vids, "File has multiple video streams. Only the first stream will be kept");
        videos.truncate(1);
    }

    if audios.is_empty() {
        // if no english streams are detected, just use all streams
        for stream in parsed.iter_mut() {
            if let Stream::Audio(ref mut x) = stream {
                audios.push(Stream::Audio(mem::take(x)));
            }
        }
    }

    if subtitles.is_empty() {
        // if no english streams are detected, just use all streams
        for stream in parsed.iter_mut() {
            if let Stream::Subtitle(ref mut x) = stream {
                subtitles.push(Stream::Subtitle(mem::take(x)));
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
    use codec::Id::AAC;
    use codec::Id::DTS;
    use codec::Id::DVD_SUBTITLE;
    use codec::Id::FLAC;
    use codec::Id::H264;
    use codec::Id::HDMV_PGS_SUBTITLE;
    use codec::Id::HEVC;
    use codec::Id::SSA;
    use codec::Id::TRUEHD;

    stream_mappings
        .iter()
        .map(|stream| {
            let index = stream.index();
            match stream {
                Stream::Video(video) => match video.codec {
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
                Stream::Audio(audio) => match audio.codec {
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
                Stream::Subtitle(subtitle) => match subtitle.codec {
                    HDMV_PGS_SUBTITLE | DVD_SUBTITLE => (index, None),
                    _ => (index, Some(SSA)),
                },
            }
        })
        .collect()
}
