#!/bin/python
import subprocess
import json
import os


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


def clean_name(filename: str):
    if filename.endswith(".mkv"):
        return filename
    return filename[:filename.rfind(".")] + ".mkv"


def main(directory: str):
    check_dir(directory)
    global episode
    global TV
    for filename in os.listdir(directory):
        parsed_info = {"video": {}, "audio": {}, "subtitle": {}}
        if not "." in filename:
            continue
        if TV:
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

        # video starts
        if len(parsed_info["video"]) > 1:
            raise KeyError("The file provided has more than one video stream")
        video_codec = "copy" if ("h264" in parsed_info["video"][0]["codec_name"]) else "libx264"
        video_mapping = [0]
        # video ends

        # audio starts
        audio_mapping = []
        try:
            if len(parsed_info["audio"]) <= 1:
                audio_mapping = list(parsed_info["audio"].keys())
            else:  # check for eng
                for k, i in parsed_info["audio"].items():
                    for v in i["tags"].values():
                        if "eng" in str(v):
                            audio_mapping.append(int(k))
                            break
        except KeyError:
            audio_mapping = list(parsed_info["audio"].keys())

        audio_mapping = list(set(audio_mapping))
        audio_mapping.sort()

        audio_codecs = {}
        for k, v in parsed_info["audio"].items():
            try:
                if ("dts" in v["profile"].lower()) or ("ma" in v["profile"].lower()) or ("truehd" in v["profile"].lower()):
                    audio_codecs[k] = "flac"
                    continue
            except KeyError:
                pass
            if "aac" in v["codec_name"]:
                audio_codecs[k] = "copy"
            else:
                audio_codecs[k] = "libfdk_aac"
        # audio ends

        # subtitle starts
        if len(parsed_info["subtitle"]) <= 1:
            subtitle_mapping = list(parsed_info["subtitle"].keys())

        subtitle_mapping = []
        if len(parsed_info["subtitle"]) <= 1:
            subtitle_mapping = list(parsed_info["subtitle"].keys())
        else:  # check for eng
            for k, i in parsed_info["subtitle"].items():
                for v in i["tags"].values():
                    if "eng" in str(v):
                        subtitle_mapping.append(int(k))
                        break
        subtitle_mapping = list(set(subtitle_mapping))
        subtitle_mapping.sort()

        subtitle_codecs = {}
        for k, v in parsed_info["subtitle"].items():
            if ("pgs" in v["codec_name"]) or ("dvd" in v["codec_name"]):
                subtitle_codecs[k] = "copy"
            else:
                subtitle_codecs[k] = "ass"
        # subtitle ends

        codec_cmds = []
        for c, i in enumerate(audio_mapping):
            codec_cmds.extend([f"-c:a:{c}", audio_codecs[i]])
        for c, i in enumerate(subtitle_mapping):
            codec_cmds.extend([f"-c:s:{c}", subtitle_codecs[i]])

        map_cmds = []
        for i in video_mapping:
            map_cmds.extend(["-map", f"0:{i}"])
        for i in audio_mapping:
            map_cmds.extend(["-map", f"0:{i}"])
        for i in subtitle_mapping:
            map_cmds.extend(["-map", f"0:{i}"])

        if TV:
            global title, season
            outname = f"{title} - s{season:02}e{episode:02}.mkv"
        else:
            outname = clean_name(filename)

        additional_cmds = codec_cmds + map_cmds
        encode(filename, f"newfiles/{outname}", video_codec=video_codec, others=additional_cmds)


if __name__ == "__main__":
    TV = "n" not in input("TV show mode? (Y/n) ")
    title = input("Please enter the title of the TV Show: ")
    season = int(input("Which season is this? "))
    episode = input("What is the first episode in this disc? (defaults to 1) ")
    episode = int(episode) - 1 if episode != "" else 0
    main(".")
