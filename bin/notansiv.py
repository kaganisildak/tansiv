#!/usr/bin/env python3

"""This script orchestrates the start of n VMs on a single node

It forks/exec as many time as needed the tanboot script (need to be on the PATH).
"""

import argparse
import logging
from pathlib import Path
import os
import signal
import sys


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
    autoconfig_net: bool,
    base_working_dir: Path,
    number: int,
):
    global child_pids
    for i in range(number):
        pid = os.fork()
        descriptor = 10 + i
        tantap_ip = f"192.168.1.{descriptor}/24"
        management_ip = f"10.0.0.{descriptor}/24"
        env = {k: v for k, v in os.environ.items()}
        arguments = [
            boot_cmd,
            # unused but we need to align with the requirement
            # that the first param is the socket name
            # (injected by tansiv in case of a ts)
            "socket_unused",
            tantap_ip,
            management_ip,
            "--qemu_cmd",
            qemu_cmd,
            "--qemu_image",
            qemu_image,
            "--qemu_args",
            qemu_args,
            "--base_working_dir",
            base_working_dir,
            "--mode",
            "tap"
        ]
        if autoconfig_net:
            arguments.append("--autoconfig_net")
        print(boot_cmd)
        print(arguments)
        if pid == 0:
            # exec
            os.execvpe(
                boot_cmd,
                arguments,
                env,
            )
        else:
            child_pids.append(pid)
    return child_pids


def main():
    signal.signal(signal.SIGINT, terminate)
    signal.signal(signal.SIGTERM, terminate)

    sys.path.append(".")

    logging.basicConfig(level=logging.DEBUG)
    parser = argparse.ArgumentParser(description="Tansiv experimentation engine")

    # This are the arguments from boot.py
    parser.add_argument("--qemu_cmd", type=str, help="qemu cmd to pass", required=True)

    parser.add_argument(
        "--qemu_mem", type=str, help="Memory to use (e.g 1g)", required=True
    )

    parser.add_argument(
        "--qemu_args",
        type=str,
        help="extra qemu args to pass (e.g.'--icount shift=1,sleep=on,align=off')",
        default="",
    )

    parser.add_argument(
        "--autoconfig_net",
        action="store_true",
        help="True iff network must be autoconfigured",
    )

    parser.add_argument("--qemu_image", type=str, help="disk image", required=True)
    parser.add_argument(
        "--boot_cmd",
        help="path to the boot.py wrapper, default to the one in the PATH (if any)",
        default="boot.py",
    )
    parser.add_argument(
        "--base_working_dir", help="base directory where the working dir will be stored. Default to /tmp.", default="/tmp"
    )

    # Specific options
    parser.add_argument("--number", type=int, help="number of vms to start", default=2)

    args = parser.parse_args()
    qemu_cmd = args.qemu_cmd
    qemu_image = args.qemu_image
    qemu_args = args.qemu_args
    boot_cmd = args.boot_cmd
    autoconfig_net = args.autoconfig_net
    number = int(args.number)
    base_working_dir = args.base_working_dir

    child_pids = notansiv(
        qemu_cmd,
        qemu_image,
        qemu_args,
        boot_cmd,
        autoconfig_net,
        base_working_dir,
        number,
    )

    for child_pid in child_pids:
        os.waitpid(child_pid, 0)


if __name__ == "__main__":
    main()
