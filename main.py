#!/bin/python
import copy
import subprocess
import json
import os
import sys


def log(i: str):
    if "-V" in sys.argv:
        with open("./videoconverter.log", "a") as f:
            f.write(i)
            f.write("\n")
        print(i)
    if "-v" in sys.argv:
        print(i)


def encode_cpu(filename: str, outname: str, video_codec: str, crf: int, audio_codec="copy", subtitle_codec="copy",
               others: list = None, tune=False, deinterlace=False, hwaccel=True):
    log(filename)
    if others is None:
        others = []
    filters = []
    command = ["ffmpeg", "-hide_banner", "-threads", "0", "-i", filename, "-max_muxing_queue_size", "16384", "-c:v",
               video_codec, "-c:a", audio_codec, "-c:s", subtitle_codec, "-cutoff", "18000", "-vbr", "5"]

    if hwaccel:
        command.insert(4, "-hwaccel")
        command.insert(5, "auto")

    if video_codec != "copy":
        command.extend(["-crf", str(crf)])

    if tune:
        command.extend(["-tune", sys.argv[sys.argv.index("--tune") + 1]])

    if deinterlace:
        filters.append("yadif")

    if video_codec == "libx264":
        command.extend(["-profile:v", "high", "-rc-lookahead", "250", "-preset", "slow"])

    if "--crop" in sys.argv:
        filters.append(sys.argv[sys.argv.index("--crop") + 1])

    if not filters == []:
        command.append("-filter:v")
        command.append(",".join(filters))

    command.extend(others)
    command.append(outname)
    print("\n")
    print(*command, "\n")
    if "--sim" in sys.argv:
        return
    subprocess.run(command)


def encode_gpu(filename: str, outname: str, video_codec: str, crf: int, audio_codec="copy", subtitle_codec="copy",
               others: list = None, deinterlace=False, hwaccel=True):
    if video_codec == "copy":
        encode_cpu(filename, outname, video_codec, crf, audio_codec, subtitle_codec, others, deinterlace)
        return

    if others is None:
        others = []

    log(filename)
    command = ["ffmpeg", "-hide_banner", "-threads", "0", "-hwaccel", "auto", "-i", filename, "-c:v", "hevc_nvenc",
               "-c:a", audio_codec, "-c:s", subtitle_codec]
    filters = ["-filter:v", "hwupload_cuda,"]

    if deinterlace:
        filters[1] += "yadif_cuda"

    if audio_codec == "libfdk_aac":
        command.extend(["-cutoff", 18000])

    command.extend(["-rc", "constqp", "-qp", str(crf), "-preset", "slow", "-profile:v", "main", "-b:v", "0", "-rc-lookahead", "32"])

    if len(filters) > 2:
        command.extend(filters)
    command.extend(others)
    command.append(outname)
    print("\n", *command, "\n")
    subprocess.run(command)


def check_dir(directory):
    global season, TV
    outdir = f"Season {season:02}" if TV else "newfiles"
    os.chdir(directory)
    mkdir(outdir)
    return outdir


def clean_name(filename: str):
    if filename.endswith(".mkv"):
        return filename
    return filename[:filename.rfind(".")] + ".mkv"


def remux_subtitles(directory: str):
    os.chdir(directory)
    mkdir()
    filelist: list = os.listdir(directory)
    for filename in copy.deepcopy(filelist):
        if filename.endswith("srt"):
            filelist.remove(filename)
    for filename in filelist:
        subprocess.call(["ffmpeg", "-i", filename, "-i", filename[:-4] + ".srt", "-c:v", "copy", "-c:a", "copy", "-c:s",
                         "copy", "-map", "0", "-map", "1", f"newfiles/{filename[:-4] + '.mkv'}"])


def mkdir(name="newfiles"):
    if not os.path.isdir(name):
        os.mkdir(name)


def main(directory: str):
    outdir = check_dir(directory)
    global episode
    global TV
    filelist: list = os.listdir(directory)
    log(filelist)
    filelist.sort(key=lambda s: s.casefold())
    log(filelist)
    exempt_strings = [".txt", ".rar", ".nfo", ".sfv", ".jpg", ".png", ".gif"]
    exempt_strings.extend([f".r{x:02}" for x in range(100)])
    for filename in filelist:
        parsed_info = {"video": {}, "audio": {}, "subtitle": {}}
        if os.path.isdir(filename):
            continue
        if not "." in filename:
            continue
        if any(ext in filename for ext in exempt_strings):
            continue
        if os.path.isdir("./" + filename):
            continue
        if TV:
            episode += 1
        file_info = json.loads(subprocess.check_output(["ffprobe", "-v", "quiet", "-print_format", "json", "-show_format", "-show_streams", filename]))
        log(file_info)
        streams: list = file_info["streams"]

        for stream in streams:
            if "video" in stream["codec_type"]:
                parsed_info["video"][stream["index"]] = stream
            if "audio" in stream["codec_type"]:
                parsed_info["audio"][stream["index"]] = stream
            if "subtitle" in stream["codec_type"]:
                parsed_info["subtitle"][stream["index"]] = stream

        for k, v in copy.deepcopy(parsed_info)["video"].items():
            if "mjpeg" in v["codec_name"] or "png" in v["codec_name"]:
                parsed_info["video"].pop(k)

        # video starts
        if len(parsed_info["video"]) > 1:
            raise KeyError("The file provided has more than one video stream")
        video_stream = list(parsed_info["video"].keys())[0]
        video_codec = "libx264"
        if "h264" in list(parsed_info["video"].values())[0]["codec_name"]:
            video_codec = "copy"
        elif "hevc" in list(parsed_info["video"].values())[0]["codec_name"]:
            video_codec = "copy"
        upscale: bool = False
        if not parsed_info["video"][video_stream]["height"] >= 700:
            if "--upscale" in sys.argv:
                upscale = True
        video_mapping = [list(parsed_info["video"].keys())[0]]
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
                if "truehd" in v["codec_name"].lower():
                    audio_codecs[k] = "flac"
                    continue
                if ("dts" in v["profile"].lower()) and ("ma" in v["profile"].lower()):
                    audio_codecs[k] = "flac"
                    continue
            except KeyError:
                pass
            if "aac" in v["codec_name"] or "flac" in v["codec_name"]:
                audio_codecs[k] = "copy"
            else:
                audio_codecs[k] = "libfdk_aac"
        # audio ends

        # subtitle starts
        subtitle_mapping = []
        if len(parsed_info["subtitle"]) <= 1:
            subtitle_mapping = list(parsed_info["subtitle"].keys())
        else:  # check for eng. if there are no eng streams, and one or more streams have no metadata, add all
            for k, i in parsed_info["subtitle"].items():
                try:
                    for v in i["tags"].values():
                        if "eng" in str(v):
                            subtitle_mapping.append(int(k))
                            break
                except KeyError:
                    continue
            if len(subtitle_mapping) == 0:
                subtitle_mapping = list(parsed_info["subtitle"].keys())

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

        log(f"{filename} -> {outname}")
        global endStr
        endStr += f"{filename} -> {outname}\n"

        additional_cmds = codec_cmds + map_cmds
        crf = int(sys.argv[sys.argv.index("--crf") + 1]) if "--crf" in sys.argv else 20
        try:
            deinterlace = "progressive" not in parsed_info["video"][video_stream]["field_order"]
        except KeyError:
            deinterlace = False
        if "--force-reencode" in sys.argv:
            video_codec = "libx264"
        if "--gpu" in sys.argv:
            encode_gpu(filename, f"{outdir}/{outname}", crf=crf, video_codec=video_codec, others=additional_cmds, deinterlace=deinterlace)
        else:
            encode_cpu(filename, f"{outdir}/{outname}", crf=crf, video_codec=video_codec, others=additional_cmds, tune=("--tune" in sys.argv), deinterlace=deinterlace, hwaccel="--nohwaccel" not in sys.argv)


try:

    if __name__ == "__main__":
        if "--subs" in sys.argv:
            remux_subtitles(".")
            exit()
        else:
            TV = "n" not in input("TV show mode? (Y/n) ").lower()
            if TV:
                title = input("Please enter the title of the TV Show: ")
                season = int(input("Which season is this? "))
                episode = input("What is the first episode in this disc? (defaults to 1) ")
                episode = int(episode) - 1 if episode != "" else 0
            endStr = "\n"
            main(".")
            print(endStr)
except KeyboardInterrupt:
    print("Exiting")
    exit()
