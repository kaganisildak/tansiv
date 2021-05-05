#!/usr/bin/env python3

import argparse
import logging
from pathlib import Path
import os
import sys


import signal


child_pids = []


def terminate(*args):
    import sys

    global child_pids
    print("TERMINATING all processes")
    for child_pid in child_pids:
        os.kill(child_pid, signal.SIGTERM)


def notansiv(
    qemu_cmd: str,
    qemu_image: str,
    qemu_args: str,
    boot_cmd: str,
    result_dir: Path,
    number: int,
):
    global child_pids
    for i in range(number):
        pid = os.fork()
        descriptor = 10 + i
        tantap_ip = f"192.168.1.{descriptor}/24"
        management_ip = f"10.0.0.{descriptor}/24"
        if pid == 0:
            # exec
            env = {k: v for k, v in os.environ.items()}
            os.execvpe(
                f"{sys.executable}",
                [
                    sys.executable,
                    boot_cmd,
                    tantap_ip,
                    management_ip,
                    "--qemu_cmd",
                    qemu_cmd,
                    "--qemu_image",
                    qemu_image,
                    "--qemu_args",
                    qemu_args,
                    "--mode",
                    "tap",
                    "--out",
                    str(result_dir / f"vm-{descriptor}.out"),
                ],
                env,
            )
        else:
            child_pids.append(pid)
    return child_pids


if __name__ == "__main__":
    signal.signal(signal.SIGINT, terminate)
    signal.signal(signal.SIGTERM, terminate)

    sys.path.append(".")
    from constants import *

    logging.basicConfig(level=logging.DEBUG)
    parser = argparse.ArgumentParser(description="Tansiv experimentation engine")

    parser.add_argument("--qemu_cmd", help="path to the qemu command", default=QEMU)
    parser.add_argument(
        "--qemu_image", help="path to the qemu image to use", default=QEMU_IMAGE
    )
    parser.add_argument(
        "--qemu_args", help="some qemu arguments to use", default=QEMU_ARGS
    )
    parser.add_argument(
        "--boot_cmd",
        help="path to the boot.py wrapper",
        default="../examples/qemus/boot.py",
    )
    parser.add_argument(
        "--result_dir", help="path to the result_dir", default="notansiv_output"
    )
    parser.add_argument("--number", type=int, help="number of vms to start", default=2)

    args = parser.parse_args()
    qemu_cmd = args.qemu_cmd
    qemu_image = args.qemu_image
    qemu_args = args.qemu_args
    boot_cmd = args.boot_cmd
    result_dir = Path(args.result_dir)
    number = int(args.number)

    result_dir.mkdir(parents=True, exist_ok=True)
    child_pids = notansiv(qemu_cmd, qemu_image, qemu_args, boot_cmd, result_dir, number)

    for child_pid in child_pids:
        os.waitpid(child_pid, 0)
