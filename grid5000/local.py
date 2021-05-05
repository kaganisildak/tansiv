from collections import defaultdict
from ipaddress import ip_interface
from pathlib import Path
import json
import logging
import os
import time
import re
import sys
from typing import Dict

import enoslib as en
from g5k import TansivHost

N = 2


class ExpEnv(object):
    """Context manager that boots n vm.

    The intent is to use it as a drop-in replacement of the tansiv process
    to test stuffs without Tansiv in the picture.
    So this will fork/exec as many VMs as needed using a call to the boot.py
    wrapper (the same as tansiv uses).  These VMs will thus use a regular
    bridged tap to communicate.

    Args:
        qemu_cmd: the path to the qemu executable
        qemu_image: the disk image to use (e.g. a qcow2 image)
        qemu_args: some qemu args to use (-icount sleep=off,shift=0 sounds reasonnable)
        result_dir: put some logs (each vm stdout) in this directory
        boot_cmd: the path to the boot.py executable that will boot the vms.
        number: number of VMs to boot
    """

    def __init__(
        self,
        qemu_cmd: str,
        qemu_image: str,
        qemu_args: str,
        result_dir: str,
        boot_cmd: str = "../examples/qemus/boot.py",
        number: int = 2,
    ):
        self.boot_cmd = boot_cmd
        self.qemu_cmd = qemu_cmd
        self.qemu_image = qemu_image
        self.qemu_args = qemu_args
        self.number = number
        self.result_dir = result_dir
        self.child_pids = []

    def __enter__(self):
        # fork exec some VMs
        for i in range(self.number):
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
                        self.boot_cmd,
                        tantap_ip,
                        management_ip,
                        "--qemu_cmd",
                        self.qemu_cmd,
                        "--qemu_image",
                        self.qemu_image,
                        "--qemu_args",
                        self.qemu_args,
                        "--out",
                        str(self.result_dir / f"out-{descriptor}.out"),
                    ],
                    env,
                )
            self.child_pids.append(pid)
        tansiv_hosts = []
        for i in range(self.number):
            descriptor = 10 + i
            management_ip = f"10.0.0.{descriptor}"
            tansiv_host = TansivHost(management_ip, descriptor, None)
            tansiv_host.extra["out"] = str(self.result_dir / f"out-{descriptor}.out")
            tansiv_hosts.append(tansiv_host)
            tansiv_roles = dict(all=tansiv_hosts)
        en.wait_for(roles=tansiv_roles)
        return tansiv_roles

    def __exit__(self, *args):
        import signal

        os.killpg(os.getpgrp(), signal.SIGTERM)


if __name__ == "__main__":
    logging.basicConfig(level=logging.DEBUG)
    qemu_argsz = [
        "-icount shift=0,sleep=off",
    ]
    force = False

    for qemu_args in qemu_argsz:
        sanitized = re.sub(r"\W+", "_", qemu_args)
        result_dir = Path(sanitized)
        if result_dir.exists():
            print(f"Skipping {result_dir} ...")
            continue
        result_dir.mkdir(exist_ok=True)

        with ExpEnv(
            os.environ["QEMU"],
            os.environ["IMAGE"],
            qemu_args,
            result_dir,
            number=2,
        ) as tansiv_roles:
            steps = dict()
            time.sleep(60)