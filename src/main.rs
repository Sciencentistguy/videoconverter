extern crate ffmpeg_next as ffmpeg;
//extern crate ffmpeg_sys_next as ffmpeg_sys;
use ffmpeg::codec::{self, Context, Parameters};
use ffmpeg::format::context::Input;
use ffmpeg::media::Type;
use std::collections::HashMap;
use std::process;

fn main() {
    match ffmpeg::init() {
        Err(x) => {
            eprintln!("Error: Could not initialise ffmpeg ({})", x);
            process::exit(1);
        }
        Ok(_) => {}
    }

    // Squelch libav* errors
    unsafe {
        ffmpeg::ffi::av_log_set_level(ffmpeg::ffi::AV_LOG_FATAL);
    }

    let file = match ffmpeg::format::input(&"/home/jamie/Videos/Inception/Inception_t16.mkv") {
        Err(x) => {
            eprintln!("Error: {}", x);
            process::exit(1);
        }
        Ok(x) => x,
    };

    let parsed = parse_stream_metadata(&file);
    let mappings = get_mappings(&parsed);
    let codecs = get_codecs(&parsed, &mappings);

    print_codec_mapping(&parsed, &codecs);
}

#[derive(Debug)]
enum StreamType {
    Video(Video),
    Audio(Audio),
    Subtitle(Subtitle),
}

#[derive(Debug)]
struct Video {
    index: usize,
    codec: codec::Id,
}

impl Video {
    pub fn new(index: usize, codec_par: Parameters) -> Video {
        let codec = codec_par.id();
        Video { index, codec }
    }
}

#[derive(Debug)]
struct Audio {
    index: usize,
    codec: codec::Id,
    lang: Option<String>,
    profile: Option<ffmpeg::codec::Profile>,
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
struct Subtitle {
    index: usize,
    codec: codec::Id,
    lang: Option<String>,
}

impl Subtitle {
    pub fn new(index: usize, codec_par: Parameters, metadata: ffmpeg::util::dictionary::Ref<'_>) -> Subtitle {
        let codec = codec_par.id();
        let lang = metadata.get("language").map(|f| f.to_string());

        Subtitle { index, codec, lang }
    }
}

fn parse_stream_metadata(file: &Input) -> Vec<StreamType> {
    let mut out: Vec<StreamType> = Vec::new();
    for stream in file.streams() {
        let index = stream.index();
        let codec = stream.codec();
        let codec_parameters = stream.parameters();
        let tags = stream.metadata();
        //let explode = codec.codec().unwrap();
        match codec.medium() {
            Type::Video => {
                out.push(StreamType::Video(Video::new(index, codec_parameters)));
            }
            Type::Audio => {
                out.push(StreamType::Audio(Audio::new(index, codec, codec_parameters, tags)));
            }
            Type::Subtitle => {
                out.push(StreamType::Subtitle(Subtitle::new(index, codec_parameters, tags)));
            }
            _ => {}
        };
    }
    return out;
}

fn get_mappings(parsed: &Vec<StreamType>) -> Vec<usize> {
    let mut video_mappings: Vec<usize> = Vec::new();
    let mut audio_mappings: Vec<usize> = Vec::new();
    let mut subtitle_mappings: Vec<usize> = Vec::new();

    for stream in parsed.iter() {
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
        eprintln!("Erorr: File has {} video streams", num_vids);
        process::exit(1);
    }

    if audio_mappings.len() == 0 { // if no english streams are detected, just use all streams
        for stream in parsed.iter() {
            match stream {
                StreamType::Audio(audio) => {
                    audio_mappings.push(audio.index);
                }
                _ => {}
            }
        }
    }

    if subtitle_mappings.len() == 0 { // if no english streams are detected, just use all streams
        for stream in parsed.iter() {
            match stream {
                StreamType::Subtitle(subtitle) => {
                    subtitle_mappings.push(subtitle.index);
                }
                _ => {}
            }
        }
    }

    return video_mappings
        .into_iter()
        .chain(audio_mappings.into_iter().chain(subtitle_mappings.into_iter()))
        .collect();
}

fn get_codecs(parsed: &Vec<StreamType>, mappings: &Vec<usize>) -> HashMap<usize, Option<codec::Id>> {
    let mut video_codecs: HashMap<usize, Option<codec::Id>> = HashMap::new();
    let mut audio_codecs: HashMap<usize, Option<codec::Id>> = HashMap::new();
    let mut subtitle_codecs: HashMap<usize, Option<codec::Id>> = HashMap::new();

    for index in mappings {
        let index = *index;
        let stream = &parsed[index];
        match stream {
            StreamType::Video(video) => match video.codec {
                codec::Id::HEVC => {
                    video_codecs.insert(index, None);
                }
                codec::Id::H264 => {
                    video_codecs.insert(index, None);
                }
                _ => {
                    video_codecs.insert(index, Some(codec::Id::H264));
                }
            },
            StreamType::Audio(audio) => match audio.codec {
                codec::Id::FLAC => {
                    audio_codecs.insert(index, None);
                }
                codec::Id::AAC => {
                    audio_codecs.insert(index, None);
                }

                codec::Id::TRUEHD => {
                    audio_codecs.insert(index, Some(codec::Id::FLAC));
                }
                codec::Id::DTS => match audio.profile {
                    Some(codec::Profile::DTS(codec::profile::DTS::HD_MA)) => {
                        audio_codecs.insert(index, Some(codec::Id::FLAC));
                    }
                    _ => {
                        audio_codecs.insert(index, Some(codec::Id::AAC));
                    }
                },
                _ => {
                    audio_codecs.insert(index, Some(codec::Id::AAC));
                }
            },
            StreamType::Subtitle(subtitle) => match subtitle.codec {
                codec::Id::HDMV_PGS_SUBTITLE => {
                    subtitle_codecs.insert(index, None);
                }
                codec::Id::DVD_SUBTITLE => {
                    subtitle_codecs.insert(index, None);
                }
                _ => {
                    subtitle_codecs.insert(index, Some(codec::Id::SSA));
                }
            },
        }
    }

    return video_codecs
        .into_iter()
        .chain(audio_codecs.into_iter().chain(subtitle_codecs.into_iter()))
        .collect();
}

fn print_codec_mapping(parsed: &Vec<StreamType>, codecs: &HashMap<usize, Option<codec::Id>>) {
    for (index, codec) in codecs.iter() {
        let oldcodec = match &parsed[*index] {
            StreamType::Video(video) => video.codec,
            StreamType::Audio(audio) => audio.codec,
            StreamType::Subtitle(subtitle) => subtitle.codec,
        };
        let newcodec = match codec {
            None => oldcodec,
            Some(x) => *x,
        };
        print!("stream {}: {:?} -> {:?}", index, oldcodec, newcodec);
        if codec.is_none() {
            println!(" (copy)");
        } else {
            println!("");
        }
    }
}
