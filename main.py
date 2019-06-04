#!/bin/python
import subprocess
import json
import os
import sys


def encode(filename, outname, video_codec="copy", crf=20, audio_codec="copy", subtitle_codec="copy", others: list = None):
    if others is None:
        others = []
    command = ["ffmpeg", "-threads", "0", "-i", filename, "-c:v", video_codec, "-c:a", audio_codec, "-c:s", subtitle_codec]
    if video_codec != "copy":
        command.extend(["-crf", str(crf)])
    command.extend(others)
    command.append(outname)
    subprocess.run(command)


def check_dir(directory):
    os.chdir(directory)
    if not os.path.isdir("newfiles"):
        os.mkdir("newfiles")


def parse_streams(streams: list):
    out = []
    for stream in streams:
        ls = []
        stream: dict = stream


def clean_name(filename: str):
    if filename.endswith(".mkv"):
        return filename
    return filename[:filename.rfind(".")] + ".mkv"


def main(directory: str):
    check_dir(directory)
    episode = 0
    for filename in os.listdir(directory):
        parsed_info = {"video": {}, "audio": {}, "subtitle": {}}
        if not "." in filename:
            continue
        episode += 1
        file_info = json.loads(subprocess.check_output(["ffprobe", "-v", "quiet", "-print_format", "json", "-show_format", "-show_streams", filename]))
        print(file_info)
        streams: list = file_info["streams"]

        for stream in streams:
            if "video" in stream["codec_type"]:
                parsed_info["video"][stream["index"]] = stream
            if "audio" in stream["codec_type"]:
                parsed_info["audio"][stream["index"]] = stream
            if "subtitle" in stream["codec_type"]:
                parsed_info["subtitle"][stream["index"]] = stream

        if len(parsed_info["video"]) > 1:
            raise KeyError("The file provided has more than one video stream")
        video_codec = "copy" if ("libx264" in parsed_info["video"][0]["codec_name"]) else "libx264"
        video_mapping = [0]
        map_cmds = []

        # if len(parsed_info["audio"]) <= 1:
        audio_mapping = list(parsed_info["audio"].keys())
        for i in parsed_info["audio"].values():
            if "aac" not in i["codec_name"]:
                audio_codec = "libfdk_aac"
                break
            audio_codec = "copy"

        # if len(parsed_info["subtitle"]) <= 1:
        subtitle_mapping = list(parsed_info["subtitle"].keys())
        subtitle_codec = "copy"

        for i in video_mapping:
            map_cmds.extend(["-map", f"0:{i}"])
        for i in audio_mapping:
            map_cmds.extend(["-map", f"0:{i}"])
        for i in subtitle_mapping:
            map_cmds.extend(["-map", f"0:{i}"])
        print("")
        global TV
        if TV:
            global title, season
            outname = f"{title} - s{season:02}e{episode:02}.mkv"
        else:
            outname = clean_name(filename)
        encode(filename, f"newfiles/{outname}", video_codec=video_codec, audio_codec=audio_codec, subtitle_codec=subtitle_codec, others=map_cmds)


if "pycharm" in sys.argv:
    main("/home/jamie/Videos/Its Always Sunny in Philadelphia Season 1, 2, 3, 4, 5 & 6 + Extras DVDRip TSV/Season 01")

if __name__ == "__main__":
    TV = "n" not in input("TV show mode? (Y/n) ")
    title = input("Please enter the title of the TV Show: ")
    season = int(input("Which season is this? "))
    main(".")
