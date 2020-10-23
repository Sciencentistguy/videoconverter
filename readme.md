# VideoConverter

## Usage
Run `main.py` with python 3, in the directory containing your media files. Use `-h` to see possible arguments.
If no arguments are given, by default the program will ask if you want to enable TV show mode. If you do, then it will ask you to provide the show name, the current season, and the first episode in the current directory. (This is useful for DVD box sets, where each disk contains some episodes but not a full season.) TV show mode will then enable [renaming](https://support.plex.tv/articles/naming-and-organizing-your-tv-show-files/), and store the output in a folder named with the season.

The program will analyse each file, and convert audio and video streams appropriately, to the following:
* Container:
    * `.mkv`
* Video:
    * If the original stream is h.264 or h.265, it will be copied.
    * Else, by default, h.264, encoded with libx264, with the following flags: `-profile:v high -rc-lookahead 250 -preset slow -crf 20`.
    * The flag `--gpu` can be passed, which enables nvenc. This produces h.265, with the following flags `-rc constqp -qp 20 -preset slow -profile:v main -b:v 0 -rc-lookahead 32`.
* Audio:
    * If the original stream is aac or flac, it will be copied
    * If the original stream is DTS-MA or Dolby TrueHD, it will be converted to flac.
    * Else, aac, encoded with libfdk_aac, with the following flags: `-cutoff 18000 -vbr 5`
* Subtitles
    * If the original stream is PGS (Bluray) or VobSub (DVD), it will be copied.
    * Else, ssa (ass), encoded with ffmpeg's built in encoder, with no special flags.

All other streams are discarded.

---
Available under the GPL
