#!/bin/python
import argparse
from argparse import Namespace
import copy
import json
import os
import subprocess
from typing import Any, List, Dict, Tuple, Union, cast

ParsedInfoType = Dict[str, Dict[int, Dict[str, Union[str, int, Dict[str, Union[str, int]]]]]]

class VideoConverter():
    def __init__(self, args: Namespace):
        self.args = args
        self.log(args)
        self.tv_mode = "n" not in input("TV show mode? (Y/n) ").lower()
        # self.epcount = 0
        loaded = self.read_position()  # title, season, epcount
        using = loaded[0] != ""
        if self.tv_mode:
            title = input(
                "Please enter the title of the TV Show: "
                if not using else f"Please enter the title of the TV Show. Leave blank to use previous ({loaded[0]})")
            if title == "":
                title = loaded[0]
                using = True
            else:
                using = False
            self.title = title
            while True:
                season: str = input(
                    "Which season is this? "
                    if not using else f"Please enter the season number. Leave blank to use previous ({loaded[1]})")
                if season == "":
                    season: str = loaded[1]
                try:
                    self.season = int(season)
                except ValueError:
                    continue
                break
            episode = input(
                f"What is the first episode in this disc? (defaults to {1 if not using else int(loaded[2])+1}) ")
            self.episode: int = int(
                episode) - 1 if episode != "" else (0 if not using else int(loaded[2]))
            self.epcount = int(self.episode)
        self.rename_log: str = "\n"

    def run(self):
        self.main(".")
        print(self.rename_log)

    def write_position_state(self):
        with open("/tmp/videoconverter", "w") as f:
            f.write(f"{self.title}\n{self.season}\n{self.epcount}")

    def read_position(self) -> Tuple[str, str, str]:
        try:
            with open("/tmp/videoconverter", "r") as f:
                ret = tuple(f.read().rstrip().split(sep="\n"))
                if len(ret) != 3:
                    raise ValueError("Malformed statefile.")
                return ret[0], ret[1], ret[2]
        except FileNotFoundError:
            return "", "", ""

    def log(self, i: Any):
        if self.args.Verbose:
            with open("./videoconverter.log", "a") as f:
                f.write(i)
                f.write("\n")
            print(i)
        elif self.args.verbose:
            print(i)

    def encode(self, filename: str, outname: str, video_codec: str, crf: int, deinterlace: bool, others: List[str] = []):
        self.log(filename)
        # others = [] if others is  else others
        filters = []

        command = ["ffmpeg", "-hide_banner"]  # Hide the GPL blurb
        # Enable hardware acceleration
        command += ["-hwaccel", "auto"] if (not self.args.no_hwaccel) else []
        command += ["-threads", "0"]  # Max CPU threads
        command += ["-i", filename,
                    "-max_muxing_queue_size", "16384"]  # Input file
        command += ["-c:v", video_codec]  # Specify video codec
        # libfdk_aac encoder settings
        command += ["-cutoff", "18000", "-vbr", "5"]
        command += ["-crf", str(crf)] if (video_codec !=
                                          "copy" and not self.args.gpu) else []  # Set CRF
        # Specify libx264 tune
        command += ["-tune",
                    self.args.tune] if (self.args.tune is not None) else []

        command += ["-profile:v", "high", "-rc-lookahead", "250", "-preset",
                    "slow"] if (video_codec == "libx264") else []  # Libx264 options
        command += ["-rc",
                    "constqp",
                    "-qp",
                    str(crf),
                    "-preset",
                    "slow",
                    "-profile:v",
                    "main",
                    "-b:v",
                    "0",
                    "-rc-lookahead",
                    "32"] if self.args.gpu else []  # nvenc options (gpu mode)
        # Crop filter
        filters += [self.args.crop] if (self.args.crop is not None) else []
        filters += (["yadif"] if not self.args.gpu else ["hwupload_cuda", "yadif_cuda"]
                    ) if deinterlace else []  # Deinterlacing filter

        # apply filters
        command += ["-filter:v", ",".join(filters)] if (filters != []) else []

        command += others
        command += [outname]
        print("\n")
        print(*command, "\n")
        if self.args.simulate:
            return
        subprocess.run(command)

    def prepare_directory(self, directory: str):
        out = f"Season {self.season:02}" if self.tv_mode else "newfiles"
        os.chdir(directory)
        self.mkdir(out)
        return out

    def clean_name(self, filename: str):
        return filename[:filename.rfind(".")] + ".mkv"

    def mkdir(self, name: str):
        if self.args.simulate:
            return
        if not os.path.isdir(name):
            os.mkdir(name)

    def analyse_video(self, parsed_info: ParsedInfoType) -> Tuple[List[int], str]:
        if len(parsed_info["video"]) > 1:
            raise ValueError(
                "The file provided has more than one video stream")
        file_video_codec = list(parsed_info["video"].values())[0]["codec_name"]
        video_codec: str = "copy" if (
            "h264" in file_video_codec or "hevc" in file_video_codec) else (
            "hevc_nvenc" if self.args.gpu else "libx264")
        if self.args.force_reencode or self.args.deinterlace:
            video_codec = "libx264"
        video_mapping = [list(parsed_info["video"].keys())[0]]
        return video_mapping, video_codec

    def analyse_audio(self, parsed_info: ParsedInfoType) -> Tuple[List[int], Dict[int, str]]:
        audio_mapping: List[int] = []
        if self.args.all_streams:
            audio_mapping = list(parsed_info["audio"].keys())
        else:
            try:
                if len(parsed_info["audio"]) <= 1:  # only one stream, use it
                    audio_mapping = list(parsed_info["audio"].keys())
                else:  # check for eng
                    for stream_index, stream in parsed_info["audio"].items():
                        for tag in cast(Dict[str, Union[str, int]], stream["tags"]).values():
                            if "eng" in str(tag):
                                audio_mapping.append(int(stream_index))
                                break
                if len(audio_mapping) == 0:  # if no english streams are found, use all streams
                    audio_mapping = list(parsed_info["audio"].keys())
            except KeyError:  # if it falls over, just use all audio streams
                audio_mapping = list(parsed_info["audio"].keys())

        audio_mapping = sorted(set(audio_mapping))

        audio_codecs = {}
        for stream_index, stream in parsed_info["audio"].items():
            try:
                # lets hope there aren't any ints in the stream info
                stream = cast(Dict[str, str], stream)
                if "truehd" in stream["codec_name"].lower() or (
                    ("dts" in stream["profile"].lower()) and (
                        "ma" in stream["profile"].lower())):
                    audio_codecs[stream_index] = "flac"
                    continue
            except KeyError:
                pass
            if "aac" in stream["codec_name"] or "flac" in stream["codec_name"]:
                audio_codecs[stream_index] = "copy"
            else:
                audio_codecs[stream_index] = "libfdk_aac"
        return audio_mapping, audio_codecs

    def analyse_subtitles(self, parsed_info: Dict[str, Dict[int, Dict[str, Union[str, int, Dict[str, Union[str, int]]]]]]) -> Tuple[List[int], Dict[int, str]]:
        subtitle_mapping = []
        if self.args.all_streams:
            subtitle_mapping = list(parsed_info["subtitle"].keys())
        else:
            if len(parsed_info["subtitle"]) <= 1:
                subtitle_mapping = list(parsed_info["subtitle"].keys())
            else:  # check for eng. if there are no eng streams, and one or more streams have no metadata, add all
                for stream_index, stream in parsed_info["subtitle"].items():
                    try:
                        for tag in cast(Dict[str, Union[str, int]], stream["tags"]).values():
                            if "eng" in str(tag):
                                subtitle_mapping.append(int(stream_index))
                                break
                    except KeyError:
                        continue
                if len(subtitle_mapping) == 0:
                    subtitle_mapping = list(parsed_info["subtitle"].keys())

        subtitle_mapping = sorted(set(subtitle_mapping))

        subtitle_codecs = {}
        for stream_index, stream in parsed_info["subtitle"].items():
            if ("pgs" in stream["codec_name"]) or ("dvd" in stream["codec_name"]):
                subtitle_codecs[stream_index] = "copy"
            else:
                subtitle_codecs[stream_index] = "ass"
        return subtitle_mapping, subtitle_codecs

    def probe_video(self, filename: str) -> Dict[str, Union[List[Dict[str, Union[str, int, Dict[str, Union[str, int]]]]], Dict[str, Union[str, int]]]]:
        return json.loads(
            subprocess.check_output(
                ["ffprobe", "-v", "quiet", "-print_format", "json", "-show_format", "-show_streams", filename]))

    def process(self, filename: str, outname: str):
        parsed_info: ParsedInfoType = {
            "video": {}, "audio": {}, "subtitle": {}}
        file_info = self.probe_video(filename)
        # print(file_info)
        # return
        self.log(file_info)
        # streams is always a list of dicts
        streams = cast(
            List[Dict[str, Union[str, int, Dict[str, Union[str, int]]]]], file_info["streams"])

        for stream in streams:
            index: int = cast(int, stream["index"])
            codec_type: str = cast(str, stream["codec_type"])
            if codec_type == "data":
                continue
            parsed_info[codec_type][index] = stream
        del file_info

        for k, v in copy.deepcopy(parsed_info["video"]).items():
            if "mjpeg" in v["codec_name"] or "png" in v["codec_name"]:
                parsed_info["video"].pop(k)

        video_mapping, video_codec = self.analyse_video(parsed_info)
        audio_mapping, audio_codecs = self.analyse_audio(parsed_info)
        subtitle_mapping, subtitle_codecs = self.analyse_subtitles(parsed_info)

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

        self.log(f"{filename} -> {outname}")
        self.rename_log += f"{filename} -> {outname}\n"

        additional_cmds = codec_cmds + map_cmds
        crf = self.args.crf if self.args.crf is not None else 20

        try:
            deinterlace = "progressive" not in parsed_info["video"][video_mapping[0]]["field_order"]
        except KeyError:
            deinterlace = False
        self.encode(
            filename,
            outname,
            crf=crf,
            video_codec=video_codec,
            others=additional_cmds,
            deinterlace=not self.args.no_deinterlace and (deinterlace or self.args.deinterlace))

    def main(self, directory: str):
        output_directory = self.prepare_directory(directory)
        filelist = os.listdir(directory)
        self.log(filelist)
        filelist.sort(key=lambda s: s.casefold())
        self.log(filelist)
        exempt_strings = [".txt", ".rar", ".nfo",
                          ".sfv", ".jpg", ".png", ".gif", ".py", ".md"]
        exempt_strings.extend([f".r{x:02}" for x in range(100)])
        for filename in filelist:
            if os.path.isdir(filename):
                continue
            if "." not in filename:
                continue
            if any(ext in filename for ext in exempt_strings):
                continue
            if os.path.isdir("./" + filename):
                continue
            if filename[0] == ".":
                continue
            if self.tv_mode:
                self.episode += 1
                outname = f"{self.title} - s{self.season:02}e{self.episode:02}.mkv"
            else:
                outname = self.clean_name(filename)
            self.process(filename, f"{output_directory}/{outname}")
        if self.tv_mode:
            self.epcount += 1
            self.write_position_state()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Convert video files")
    parser.add_argument("-a", "--all-streams", action="store_true",
                        help="Keep all streams, regardless of language metadata.")
    parser.add_argument("--crf", type=int, help="Specify a CRF value.")
    parser.add_argument(
        "-c",
        "--crop",
        type=str,
        help="Specify a crop filter. These are of the format 'crop=height:width:x:y'.")
    parser.add_argument("-d", "--deinterlace", action="store_true",
                        help="Force deinterlacing of video.")
    parser.add_argument("-D", "--no-deinterlace", action="store_true",
                        help="Disable deinterlacing of video.")
    parser.add_argument("--force-reencode", action="store_true",
                        help="Force a reencode, even if it is not needed.")
    parser.add_argument(
        "-g",
        "--gpu",
        action="store_true",
        help="Uuse GPU accelerated encoding (nvenc). This produces h.265.")
    parser.add_argument("--no-hwaccel", action="store_true",
                        help="Disable hardware accelerated decoding.")
    parser.add_argument("-s", "--simulate", action="store_true",
                        help="Do everything appart from run the ffmpeg command")
    parser.add_argument(
        "-t",
        "--tune",
        type=str,
        help="Specify libx264 tune. Options are: 'film animation grain stillimage psnr ssim fastdecode zerolatency'. Does not work with GPU mode.")
    parser.add_argument("-v", "--verbose",
                        action="store_true", help="Verbose mode.")
    parser.add_argument("-V", "--Verbose", action="store_true",
                        help="Verbose mode with a logfile.")
    args = parser.parse_args()

    vc = VideoConverter(args)
    vc.run()
