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
