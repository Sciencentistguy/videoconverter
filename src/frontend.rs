pub use ffmpeg::codec::{Context, Parameters};
pub use ffmpeg::codec;
pub use ffmpeg::format::context::Input;
pub use ffmpeg::media::Type;
use log::{error, info, warn};
use std::collections::HashMap;

#[derive(Debug)]
pub enum StreamType {
    Video(Video),
    Audio(Audio),
    Subtitle(Subtitle),
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

impl Subtitle {
    pub fn new(index: usize, codec_par: Parameters, metadata: ffmpeg::util::dictionary::Ref<'_>) -> Subtitle {
        let codec = codec_par.id();
        let lang = metadata.get("language").map(|f| f.to_string());

        Subtitle { index, codec, lang }
    }
}

pub fn parse_stream_metadata(file: &Input) -> Vec<StreamType> {
    let mut out: Vec<StreamType> = Vec::new();
    for stream in file.streams() {
        let index = stream.index();
        let codec_context = stream.codec();
        let codec_parameters = stream.parameters();
        let tags = stream.metadata();
        //let explode = codec.codec().unwrap();
        match codec_context.medium() {
            Type::Video => {
                out.push(StreamType::Video(Video::new(index, codec_context, codec_parameters)));
            }
            Type::Audio => {
                out.push(StreamType::Audio(Audio::new(index, codec_context, codec_parameters, tags)));
            }
            Type::Subtitle => {
                out.push(StreamType::Subtitle(Subtitle::new(index, codec_parameters, tags)));
            }
            _ => {}
        };
    }
    return out;
}

pub fn get_mappings(parsed: &[StreamType]) -> Vec<usize> {
    let mut video_mappings: Vec<usize> = Vec::new();
    let mut audio_mappings: Vec<usize> = Vec::new();
    let mut subtitle_mappings: Vec<usize> = Vec::new();

    for stream in parsed {
        match stream {
            StreamType::Video(video) => {
                video_mappings.push(video.index);
            }
            StreamType::Audio(audio) => {
                if audio.lang == Some("eng".to_string()) {
                    audio_mappings.push(audio.index);
                }
            }
            StreamType::Subtitle(subtitle) => {
                if subtitle.lang == Some("eng".to_string()) {
                    subtitle_mappings.push(subtitle.index);
                }
            }
        }
    }

    if video_mappings.len() != 1 {
        let num_vids = video_mappings.len();
        warn!("File has {} video streams", num_vids);
        //return Err(SimpleError::new(format!("File has {} video streams", num_vids)));
    }

    if audio_mappings.len() == 0 {
        // if no english streams are detected, just use all streams
        for stream in parsed {
            match stream {
                StreamType::Audio(audio) => {
                    audio_mappings.push(audio.index);
                }
                _ => {}
            }
        }
    }

    if subtitle_mappings.len() == 0 {
        // if no english streams are detected, just use all streams
        for stream in parsed.iter() {
            match stream {
                StreamType::Subtitle(subtitle) => {
                    subtitle_mappings.push(subtitle.index);
                }
                _ => {}
            }
        }
    }

    video_mappings
        .into_iter()
        .chain(audio_mappings.into_iter())
        .chain(subtitle_mappings.into_iter())
        .collect()
}

pub fn get_codecs(parsed: &[StreamType], mappings: &[usize]) -> HashMap<usize, Option<codec::Id>> {
    use codec::Id::{AAC, DTS, DVD_SUBTITLE, FLAC, H264, HDMV_PGS_SUBTITLE, HEVC, SSA, TRUEHD};
    mappings
        .iter()
        .map(|&index| match &parsed[index] {
            StreamType::Video(video) => match video.codec {
                HEVC | H264 => (index, None),
                _ => (index, Some(H264)),
            },
            StreamType::Audio(audio) => match audio.codec {
                FLAC | AAC => (index, None),

                TRUEHD => (index, Some(FLAC)),
                DTS => match audio.profile {
                    Some(codec::Profile::DTS(codec::profile::DTS::HD_MA)) => (index, Some(FLAC)),
                    _ => (index, Some(AAC)),
                },
                _ => (index, Some(AAC)),
            },
            StreamType::Subtitle(subtitle) => match subtitle.codec {
                HDMV_PGS_SUBTITLE | DVD_SUBTITLE => (index, None),
                _ => (index, Some(SSA)),
            },
        })
        .collect()
}

