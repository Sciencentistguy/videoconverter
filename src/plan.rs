//! `--plan`: what a conversion would do, for programs that drive videoconverter.

use std::{collections::HashMap, path::Path};

use ffmpeg::codec;
use serde::Serialize;

use crate::input::{Stream, StreamMappings};

#[derive(Serialize)]
struct Plan<'a> {
    input: &'a Path,
    output: &'a Path,
    video: Vec<Planned>,
    audio: Vec<Planned>,
    subtitles: Vec<Planned>,
}

/// One stream the output keeps, in output order.
#[derive(Serialize)]
struct Planned {
    /// Which input the stream comes from: 0 for the video file, then any subtitle files.
    file: usize,
    /// The stream's index within that input, as ffprobe numbers it.
    index: usize,
    /// The codec it will have in the output, by ffmpeg's name for it.
    codec: &'static str,
    /// Whether it is copied as it is rather than re-encoded.
    copy: bool,
}

/// The JSON line describing how `input` would be converted to `output`.
pub fn describe(
    input: &Path,
    output: &Path,
    mappings: &StreamMappings,
    codecs: &HashMap<(usize, usize), Option<codec::Id>>,
) -> String {
    let planned = |streams: &[Stream]| {
        streams
            .iter()
            .map(|stream| {
                let target = codecs.get(&(stream.file(), stream.index())).copied().flatten();
                Planned {
                    file: stream.file(),
                    index: stream.index(),
                    codec: target.unwrap_or(stream.codec()).name(),
                    copy: target.is_none(),
                }
            })
            .collect()
    };
    let plan = Plan {
        input,
        output,
        video: planned(&mappings.video),
        audio: planned(&mappings.audio),
        subtitles: planned(&mappings.subtitle),
    };
    serde_json::to_string(&plan).expect("a plan always serialises")
}
