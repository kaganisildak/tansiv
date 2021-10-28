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
    result_dir: Path,
    number: int,
    qemu_args_0: str,
):
    global child_pids
    for i in range(number):
        _qemu_args = qemu_args
        if i == 0:
            _qemu_args += " " + qemu_args_0
        pid = os.fork()
        descriptor = 10 + i
        tantap_ip = f"192.168.1.{descriptor}/24"
        management_ip = f"10.0.0.{descriptor}/24"
        env = {k: v for k, v in os.environ.items()}
        arguments = [
            boot_cmd,
            tantap_ip,
            management_ip,
            "--qemu_cmd",
            qemu_cmd,
            "--qemu_image",
            qemu_image,
            "--qemu_args",
            _qemu_args,
            "--mode",
            "tap",
            "--out",
            str(result_dir / f"vm-{descriptor}.out"),
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
        "--result_dir", help="path to the result_dir", default="notansiv_output"
    )

    # Specific options
    parser.add_argument("--number", type=int, help="number of vms to start", default=2)
    parser.add_argument(
        "--qemu_args_0",
        type=str,
        help="add some more arges on the first vm only",
        default="",
    )

    args = parser.parse_args()
    qemu_cmd = args.qemu_cmd
    qemu_image = args.qemu_image
    qemu_args = args.qemu_args
    qemu_args_0 = args.qemu_args_0
    boot_cmd = args.boot_cmd
    autoconfig_net = args.autoconfig_net
    result_dir = Path(args.result_dir)
    number = int(args.number)

    result_dir.mkdir(parents=True, exist_ok=True)
    child_pids = notansiv(
        qemu_cmd,
        qemu_image,
        qemu_args,
        boot_cmd,
        autoconfig_net,
        result_dir,
        number,
        qemu_args_0,
    )

    for child_pid in child_pids:
        os.waitpid(child_pid, 0)


if __name__ == "__main__":
    main()
