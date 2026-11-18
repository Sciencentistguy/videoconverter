# videoconverter

## Installation

The recommended installation method is with nix. This repository is a flake, and a `default.nix` is provided for flexibility.

Alternatively, `cargo install` should work provided you have `ffmpeg` installed in a way that `pkg-config` can see.

## Usage

`videoconverter [path]`.

`[path]` is optional, and defaults to `.`

Run `videoconverter -h` to see all possible arguments and their defaults.

If TV show mode is enabled, the program will ask for the following:

- The show name
- The current season
- The first episode in the current directory. (This one is useful for when you've got a season of a show split amongst multiple directories, i.e. a DVD box set)

The program will attempt to read the previous values of these from a statefile (by default `/tmp/videoconverter.state`). If this is present it will suggest these to you as default values.

## Driving videoconverter from other programs

These options let another program, such as [reel](https://github.com/Sciencentistguy/reel), run videoconverter without anyone at the keyboard.

### Converting to exact paths

`--output FILE` (`-o`) converts an input to exactly that path, without asking about TV mode or renaming, and without the `newfiles` or TV output directories. Give one `--output` per input, in the same order as the inputs; each input must be a file, and each is given once. Several inputs convert in parallel, as usual:

```sh
videoconverter --yes --no-external-subs -o out/a.mkv -o out/b.mkv -- in/a.mp4 in/b.mp4
```

`--yes` skips the confirmation, and `--no-external-subs` leaves subtitle files next to an input alone instead of muxing them in. Missing output directories are created.

With `--output`:

- An input that cannot be read (missing, not a video, no video stream) is reported on stderr and skipped, and the others are still converted.
- A failed or interrupted conversion's output is deleted, unless the file was there before the run (one you agreed to overwrite). Every output that a run creates and leaves behind is complete.
- The exit status is:

  | Status | Meaning |
  | --- | --- |
  | 0 | Every input was converted |
  | 130 | Interrupted with Ctrl-C; outputs that had finished remain |
  | other non-zero | At least one input was not converted; the rest were |

  If videoconverter is killed by a signal instead of exiting, its outputs should not be trusted.

### Plans: `--plan`

`--plan` (which needs `--output`) works out each input's conversion as usual, with the same stream selection and codec choices that the other options give, then prints it instead of converting. No output is written, no directories are created, and the statefile is not updated.

Each input that can be planned produces one line on stdout holding a single JSON object:

```json
{
  "input": "/ingest/download/Film.2160p.mkv",
  "output": "/media/Films/.Film (2000).partial.mkv",
  "video": [{"file": 0, "index": 0, "codec": "hevc", "copy": true}],
  "audio": [
    {"file": 0, "index": 2, "codec": "flac", "copy": true},
    {"file": 0, "index": 4, "codec": "flac", "copy": false}
  ],
  "subtitles": [{"file": 0, "index": 5, "codec": "hdmv_pgs_subtitle", "copy": true}]
}
```

(shown here spread over several lines; each object is printed on a single line)

| Field | Meaning |
| --- | --- |
| `input` | The input, as an absolute path with symlinks resolved |
| `output` | The `--output` given for it, as given |
| `video`, `audio`, `subtitles` | The streams the output keeps, in the order they will have in it. `video` holds at most one stream. Audio and subtitles are already reordered by `--default-audio-language` and `--default-subtitle-language` (and `--anime`): when those options find their stream, it comes first and will be marked default |

Each stream:

| Field | Meaning |
| --- | --- |
| `file` | Which input it comes from: `0` for the video file, then `1`, `2`, … for subtitle files found next to it, in the order they were found. Always `0` with `--no-external-subs` |
| `index` | Its index within that file, counting streams of every type, as ffprobe numbers them (ffprobe's `index`) |
| `codec` | The codec it will have in the output, by ffmpeg's short name (ffprobe's `codec_name`, such as `hevc`, `flac` or `ass`) |
| `copy` | `true` if it is copied unchanged, `false` if it is re-encoded to `codec` |

Attachment and data streams (such as fonts), which are kept unless `--discard-attachments` is given, are not listed.

To read the output:

- Lines come in order of the inputs' file names, not their order on the command line, so match them to inputs by `input`.
- Log messages may also be written to stdout, so ignore lines that are not JSON objects of this shape.
- Ignore fields you don't know about; new ones may be added.
- An input that cannot be planned gets no line. It is reported on stderr, and the exit status is then non-zero once the other inputs have been planned.

## Output

The program will analyse each file, and convert audio and video streams appropriately, to the following:

- Container:
  - `.mkv`
- Video:
  - If the original stream is `h.264` or `h.265`, it will be copied.
  - If GPU mode is enabled (`--gpu`), the stream will be encoded as `h.265` (nvenc) with the following flags: `-rc constqp -qp 20 -preset slow -profile:v main -b:v 0 -rc-lookahead 32`.
  - Else, it will be encoded as `h.264` (libx264) with the following flags: `-profile:v high -rc-lookahead 250 -preset slow -crf 20 -x264opts opencl`.
- Audio:
  - If the original stream is `aac` or `flac`, it will be copied.
  - If the original stream is `DTS-MA` or `Dolby TrueHD`, it will be encoded as `flac`.
  - Else, it will be encoded as `aac` (libfdk_aac) with the following flags: `-cutoff 18000 -vbr 5`.
- Subtitles
  - If the original stream is HDMV_PGS (Bluray) or DVD, it will be copied.
  - Else, it will be encoded as ssa (ass).

If there are English audio and subtitle streams, then other languages' streams will be discarded. This can be overridden with `--all-streams`.

If the file contains more than one video stream, only the first will be kept. If it contains zero video streams, the program will panic.

---

Available under the terms of version 3 of the GNU GPL.
