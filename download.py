#!/bin/python
import argparse
import os
import subprocess


def download_file(uri):
    old_dir = os.getcwd()
    os.chdir(os.path.expanduser("~/Videos/"))
    subprocess.run(["tmux", "new", "-d", "aria2c", uri])
    os.chdir(old_dir)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("target", help="The file to download", type=str)
    args = parser.parse_args()
    download_file(args.target)
