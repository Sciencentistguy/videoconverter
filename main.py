import subprocess


def encode(filename, outname, video_codec="copy", crf=20, audio_codec="copy", subtitle_codec="copy", others=None):
    if others is None:
        others = []
    command = ["ffmpeg", "-threads", "0", "-i", filename, "-c:v", video_codec, "-c:a", audio_codec, "-c:s", subtitle_codec]
    if video_codec != "copy":
        command.extend(["-crf", str(crf)])
    command.extend(others)
    command.append(outname)
    subprocess.run(command)


