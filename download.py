#!/bin/python
import argparse
import os
import subprocess


def main(magnet):
    old_dir = os.getcwd()
    os.chdir(os.path.expanduser("~/Videos/"))
    subprocess.run(["tmux", "new", "-d", "aria2c", magnet])
    os.chdir(old_dir)


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("target", help="The file to download", type=str)
    args = parser.parse_args()
    main(args.magnet)
