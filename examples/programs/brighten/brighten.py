#!/usr/bin/env python3
import json
import subprocess
import sys


def fail(message: str) -> None:
    print(message, file=sys.stderr)
    raise SystemExit(2)


request = json.load(sys.stdin)
if request.get("protocol_version") != 1:
    fail("unsupported ClipAsm external protocol")

amount = request["parameters"].get("amount")
if not isinstance(amount, int) or not -100 <= amount <= 100:
    fail("amount must be an integer from -100 to 100")

video = request["inputs"]["video"]
project = request["project"]
ffmpeg = request["tools"]["ffmpeg"]
output = request["output"]
brightness = amount / 100.0
fps = project["video"]["fps"]

subprocess.run(
    [
        ffmpeg,
        "-y",
        "-v",
        "error",
        "-i",
        video["path"],
        "-filter_complex",
        f"[0:v]eq=brightness={brightness}[v]",
        "-map",
        "[v]",
        "-map",
        "0:a:0",
        "-frames:v",
        str(video["domain"]["frames"]),
        "-c:v",
        "ffv1",
        "-level",
        "3",
        "-pix_fmt",
        "yuv444p",
        "-r",
        f"{fps['numerator']}/{fps['denominator']}",
        "-c:a",
        "flac",
        "-ar",
        str(project["audio"]["sample_rate"]),
        "-ac",
        str(project["audio"]["channels"]),
        output,
    ],
    check=True,
)
