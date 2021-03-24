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
VM1 = ip_interface("10.0.0.11/24")
VM2 = ip_interface("10.0.0.12/24")
VMS = [VM1, VM2]


class ExpEnv(object):
    """Context manager that boots n vm."""

    def __init__(self, qemu_cmd, qemu_image, qemu_args, result_dir, number: int = 2):
        self.qemu_cmd = qemu_cmd
        self.qemu_image = qemu_image
        self.qemu_args = qemu_args
        self.number = number
        self.result_dir = result_dir
        self.child_pids = []

    def __enter__(self):
        # fork exec 2 vms
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
                        "../examples/qemus/boot.py",
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


def flent(tansiv_roles, result_dir=Path("result"), bench: str = "tcp_download"):
    hosts = tansiv_roles["all"]
    # split the hosts
    masters = hosts[0:2:]
    workers = hosts[1:2:]
    vm_time = defaultdict(list)
    host_time = defaultdict(list)
    for machine in hosts:
        machine.extra.update(flent_server=masters[0].extra["tansiv_alias"])

    with en.play_on(roles=dict(all=masters)) as p:
        p.shell(
            "(tmux ls | grep netserver ) ||tmux new-session -s netserver -d 'netserver -D'"
        )

    with en.play_on(roles=dict(all=workers)) as p:
        p.shell(
            (
                f"flent {bench}"
                + " -p totals -l 30 -H {{ flent_server }} "
                + f"-o {bench}.png"
            )
        )
        p.fetch(src=f"{bench}.png", dest=str(result_dir))

    return {bench: dict(vm_time=vm_time, host_time=host_time)}


def stress(tansiv_roles, args):

    vm_time = defaultdict(list)
    host_time = defaultdict(list)
    with en.play_on(roles=tansiv_roles) as p:
        p.shell(f"stress {args}")

    return {args: dict(vm_time=vm_time, host_time=host_time)}


def dump_steps(result_dir: Path, args: str, stepŝ: Dict):
    with (result_dir / "check_timers.json").open("w") as f:
        json.dump(dict(qemu_args=args, results=steps), f)


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

            for stress_cmd in [
                "--cpu 1 --timeout 30s",
                #             "--io 1 --timeout 30s",
                "--hdd 1 --timeout 30s",
            ]:
                steps.update(stress(tansiv_roles, stress_cmd))
                print(steps)
                dump_steps(result_dir, qemu_args, steps)
                time.sleep(30)

            for bench in ["tcp_download", "tcp_upload", "udp_flood"]:
                steps.update(flent(tansiv_roles, result_dir, bench=bench))
                # wait some time (time as measured on the host system)
                dump_steps(result_dir, qemu_args, steps)
                time.sleep(30)

            time.sleep(60)
