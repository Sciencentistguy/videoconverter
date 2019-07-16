#!/bin/python
import os
import subprocess

def wavtoflac():
    try:
        os.mkdir("newfiles")
    except FileExistsError:
        pass
    for file in os.listdir("."):
        if not "wav" in file:
            continue
        subprocess.run(["ffmpeg","-hide_banner","-i",file,"-c:a", "flac",f"newfiles/{file[:-4]}.flac"])


if __name__=="__main__":
    wavtoflac()
