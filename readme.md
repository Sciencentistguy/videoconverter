# VideoConverter

## Installation

Install with `cargo install --git https://github.com/Sciencentistguy/videoconverter.git`

There is also a `PKGBUILD` provided.

## Usage

`videoconverter [path]`.

`[path]` is optional, and defaults to `.`

Run `videoconverter -h` to see all possible arguments and their defaults.

By default the program will ask if you want to enable TV show mode. If you do, then it will ask you to provide the show name, the current season, and the first episode in the current directory. (This is useful for DVD box sets, where each disk contains some episodes but not a full season.) TV show mode will then enable [renaming](https://support.plex.tv/articles/naming-and-organizing-your-tv-show-files/), and store the output in a folder named with the season.

## Output

The program will analyse each file, and convert audio and video streams appropriately, to the following:

- Container:
  - `.mkv`
- Video:
  - If the original stream is `h.264` or `h.265`, it will be copied.
  - If GPU mode is enabled (`--gpu`), the stream will be encoded as `h.265` (nvenc), with the following flags: `-rc constqp -qp 20 -preset slow -profile:v main -b:v 0 -rc-lookahead 32`.
  - Else, it will be encoded as `h.264` (libx264), with the following flags: `-profile:v high -rc-lookahead 250 -preset slow -crf 20 -x264opts opencl`.
- Audio:
  - If the original stream is `aac` or `flac`, it will be copied.
  - If the original stream is `DTS-MA` or `Dolby TrueHD`, it will be encoded as `flac`.
  - Else, it will be encoded as `aac` (libfdk_aac), with the following flags: `-cutoff 18000 -vbr 5`.
- Subtitles
  - If the original stream is HDMV_PGS (Bluray) or DVD, it will be copied.
  - Else, ssa (ass), encoded with ffmpeg's built in encoder, with no special flags.

All other streams are discarded.

## Info

This program uses `libav*` from the [ffmpeg](https://ffmpeg.org/) project to analyse the input files. It then constructs a command for `ffmpeg` to convert the files, and then runs it.

## TODO

- Write a backend that uses `libav*` directly.

---

Available under the terms of version 3 of the GNU GPL.
