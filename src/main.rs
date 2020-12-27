extern crate ffmpeg_next as ffmpeg;
//extern crate ffmpeg_sys_next as ffmpeg_sys;
use ffmpeg::codec::{self, Context, Parameters};
use ffmpeg::format::context::Input;
use ffmpeg::media::Type;
use std::collections::HashMap;
use std::io;
use std::process;

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
    pub fn new(index: usize, codec_par: Parameters) -> io::Result<Video> {
        //let video = codex_ctx.decoder().video()?;
        let codec = codec_par.id();
        Ok(Video { index, codec })
    }
}

#[derive(Debug)]
struct Audio {
    index: usize,
    codec: codec::Id,
    lang: Option<String>,
    //profile: Option<String>,
}

impl Audio {
    pub fn new(index: usize, codec_context: Context, codec_par: Parameters, metadata: ffmpeg::util::dictionary::Ref<'_>) -> io::Result<Audio> {
        let codec = codec_par.id();
        let lang = metadata.get("language").map(|f| f.to_string());
        let profile = codec_context.codec().as_ref().map(|x| x.profiles());

        //ffmpeg_sys_next::
        if profile.is_some() {
            let subprof = profile.unwrap();
            if subprof.is_some() {
                let p = subprof.unwrap();
                for i in p {
                    println!("{:?}", i);
                }
            }
        }

        Ok(Audio { index, codec, lang })
    }
}

#[derive(Debug)]
struct Subtitle {
    index: usize,
    codec: codec::Id,
    lang: Option<String>,
}

impl Subtitle {
    pub fn new(index: usize, codec: Context, codec_par: Parameters, metadata: ffmpeg::util::dictionary::Ref<'_>) -> io::Result<Subtitle> {
        let codec = codec_par.id();
        let lang = metadata.get("language").map(|f| f.to_string());

        Ok(Subtitle { index, codec, lang })
    }
}

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

    let parsed = match parse_stream_metadata(file) {
        Ok(x) => x,
        Err(x) => {
            eprint!("Error: {}", x);
            process::exit(1);
        }
    };

    let mut video_mappings: Vec<usize> = Vec::new();
    let mut audio_mappings: Vec<usize> = Vec::new();
    let mut subtitle_mappings: Vec<usize> = Vec::new();

    let mut audio_codecs: HashMap<usize, codec::Id> = HashMap::new();

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

        println!("{:?}", stream);
    }
    println!("{:?}", video_mappings);
    println!("{:?}", audio_mappings);
    println!("{:?}", subtitle_mappings);

    let mappings: Vec<usize> = video_mappings
        .into_iter()
        .chain(audio_mappings.into_iter().chain(subtitle_mappings.into_iter()))
        .collect();
    println!("{:?}", mappings);
}

fn parse_stream_metadata(file: Input) -> io::Result<Vec<StreamType>> {
    let mut out: Vec<StreamType> = Vec::new();
    for stream in file.streams() {
        let index = stream.index();
        let codec = stream.codec();
        let codec_parameters = stream.parameters();
        let tags = stream.metadata();
        let explode = codec.codec().unwrap();
        match codec.medium() {
            Type::Video => {
                out.push(StreamType::Video(Video::new(index, codec_parameters)?));
            }
            Type::Audio => {
                out.push(StreamType::Audio(Audio::new(index, codec, codec_parameters, tags)?));
            }
            Type::Subtitle => {
                out.push(StreamType::Subtitle(Subtitle::new(index, codec, codec_parameters, tags)?));
            }
            _ => {}
        };
    }
    return Ok(out);
}
