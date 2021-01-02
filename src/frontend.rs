use crate::interface::Opt;

pub use ffmpeg::codec;
pub use ffmpeg::codec::{Context, Parameters};
pub use ffmpeg::format::context::Input;
pub use ffmpeg::media::Type;
use log::{error, warn};
use std::collections::HashMap;

pub struct StreamMappings {
    pub video: Vec<Stream>,
    pub audio: Vec<Stream>,
    pub subtitle: Vec<Stream>,
}

impl StreamMappings {
    pub fn iter(&self) -> impl Iterator<Item = &Stream> {
        self.video.iter().chain(self.audio.iter()).chain(self.subtitle.iter())
    }
}

#[derive(Debug)]
pub enum Stream {
    Video(Video),
    Audio(Audio),
    Subtitle(Subtitle),
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

impl Default for Video {
    fn default() -> Self {
        Video {
            index: 0,
            codec: codec::Id::None,
            field_order: FieldOrder::Unknown,
        }
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
                error!("Error getting field order for stream {}: {:?}", index, x);
                FieldOrder::Unknown
            }
        };

        Video { index, codec, field_order }
    }
}

#[derive(Debug)]
pub struct Audio {
    pub index: usize,
    pub codec: codec::Id,
    pub lang: Option<String>,
    pub profile: Option<ffmpeg::codec::Profile>,
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

impl Audio {
    pub fn new(index: usize, codec_context: Context, codec_par: Parameters, metadata: ffmpeg::util::dictionary::Ref<'_>) -> Audio {
        let codec = codec_par.id();
        let lang = metadata.get("language").map(|f| f.to_string());
        let decoder = codec_context.decoder().audio();
        let profile = match decoder.map(|x| x.profile()) {
            Ok(codec::Profile::Unknown) => None,
            Ok(x) => Some(x),
            Err(_) => None,
        };

        Audio { index, codec, lang, profile }
    }
}

#[derive(Debug)]
pub struct Subtitle {
    pub index: usize,
    pub codec: codec::Id,
    pub lang: Option<String>,
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

impl Subtitle {
    pub fn new(index: usize, codec_par: Parameters, metadata: ffmpeg::util::dictionary::Ref<'_>) -> Subtitle {
        let codec = codec_par.id();
        let lang = metadata.get("language").map(|f| f.to_string());

        Subtitle { index, codec, lang }
    }
}

pub fn parse_stream_metadata(file: &Input) -> Vec<Stream> {
    let mut out: Vec<Stream> = Vec::new();
    for stream in file.streams() {
        let index = stream.index();
        let codec_context = stream.codec();
        let codec_parameters = stream.parameters();
        let tags = stream.metadata();
        //let explode = codec.codec().unwrap();
        match codec_context.medium() {
            Type::Video => {
                out.push(Stream::Video(Video::new(index, codec_context, codec_parameters)));
            }
            Type::Audio => {
                out.push(Stream::Audio(Audio::new(index, codec_context, codec_parameters, tags)));
            }
            Type::Subtitle => {
                out.push(Stream::Subtitle(Subtitle::new(index, codec_parameters, tags)));
            }
            _ => {}
        };
    }
    return out;
}

pub fn get_stream_mappings(parsed: &mut [Stream], args: &Opt) -> StreamMappings {
    use std::mem::take;
    let mut video: Vec<Stream> = Vec::new();
    let mut audio: Vec<Stream> = Vec::new();
    let mut subtitle: Vec<Stream> = Vec::new();
    //let mut audio_mappings: Vec<usize> = Vec::new();
    //let mut subtitle_mappings: Vec<usize> = Vec::new();

    for stream in parsed.into_iter() {
        match stream {
            Stream::Video(x) => {
                video.push(Stream::Video(take(x)));
            }
            Stream::Audio(x) => {
                if x.lang == Some("eng".to_string()) || args.all_streams {
                    audio.push(Stream::Audio(take(x)));
                    //audio_mappings.push(audio.index);
                }
            }
            Stream::Subtitle(x) => {
                if x.lang == Some("eng".to_string()) || args.all_streams {
                    subtitle.push(Stream::Subtitle(take(x)));
                    //subtitle_mappings.push(subtitle.index);
                }
            }
        }
    }

    if video.len() != 1 {
        let num_vids = video.len();
        warn!("File has {} video streams", num_vids);
        //return Err(SimpleError::new(format!("File has {} video streams", num_vids)));
    }

    if audio.len() == 0 {
        // if no english streams are detected, just use all streams
        for stream in parsed.into_iter() {
            if let Stream::Audio(x) = stream {
                audio.push(Stream::Audio(take(x)));
            }
        }
    }

    if subtitle.len() == 0 {
        // if no english streams are detected, just use all streams
        for stream in parsed.into_iter() {
            if let Stream::Subtitle(x) = stream {
                subtitle.push(Stream::Subtitle(take(x)));
            }
        }
    }

    StreamMappings { video, audio, subtitle }
}

pub fn get_codec_mapping(stream_mappings: &StreamMappings, args: &crate::interface::Opt) -> HashMap<usize, Option<codec::Id>> {
    use codec::Id::{AAC, DTS, DVD_SUBTITLE, FLAC, H264, HDMV_PGS_SUBTITLE, HEVC, SSA, TRUEHD};

    stream_mappings
        .iter()
        .map(|stream| {
            let index = stream.index();
            match stream {
                Stream::Video(video) => match video.codec {
                    HEVC | H264 => (index, None),
                    _ => (index, Some(if args.gpu { HEVC } else { H264 })),
                },
                Stream::Audio(audio) => match audio.codec {
                    FLAC | AAC => (index, None),

                    TRUEHD => (index, Some(FLAC)),
                    DTS => match audio.profile {
                        Some(codec::Profile::DTS(codec::profile::DTS::HD_MA)) => (index, Some(FLAC)),
                        _ => (index, Some(AAC)),
                    },
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
