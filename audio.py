#!/bin/python
from main import mkdir
import os
import subprocess


def wavtoflac():
    mkdir()
    for file in os.listdir("."):
        if ".wav" not in file:
            continue
        subprocess.run(["ffmpeg", "-hide_banner", "-i", file, "-c:a", "flac", f"newfiles/{file[:-4]}.flac"])


def stripflac():
    mkdir()
    for file in os.listdir("."):
        subprocess.run(["ffmpeg", "-hide_banner", "-i", file, "-c:a", "copy", "-map_metadata", "-1", f"newfiles/{file}"])


if __name__ == "__main__":
    wavtoflac()
