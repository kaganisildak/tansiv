#!/usr/bin/env python3
import argparse
from ipaddress import IPv4Interface
import logging
from pathlib import Path
import os
from subprocess import check_call
import tempfile
from typing import Dict, List, Optional
import yaml

# some env variable supported by the program
ENV_QEMU = "QEMU"
ENV_IMAGE = "IMAGE"

LOGGER = logging.getLogger(__name__)


def from_env(key: str) -> str:
    value = os.environ.get(key)
    if value is None:
        raise ValueError(f"Missing {key} in the environment")
    return value


class TansivVM(object):
    def __init__(
        self,
        ip_tantap: IPv4Interface,
        ip_management: IPv4Interface,
        qemu_cmd: str,
        qemu_image: Path(),
        qemu_args: Optional[str] = None,
    ):
        self.tantap = ip_tantap
        self.management = ip_management
        self.qemu_cmd = qemu_cmd
        self.qemu_image = qemu_image.resolve()

        if qemu_args is None:
            self.qemu_args = " --icount shift=1,sleep=on" " -rtc clock=vm" " -m 1g"
        else:
            self.qemu_args = args

    @property
    def hostname(self) -> str:
        return f"tansiv-{str(self.tantap.ip).replace('.', '-')}"

    @property
    def tapname(self) -> List[str]:
        _, _, _, t = self.tantap.ip.packed
        _, _, _, m = self.management.ip.packed
        return [f"tantap{t}", f"mantap{m}"]

    @property
    def mac(self) -> List[str]:
        _, _, _, t = self.tantap.ip.packed
        _, _, _, m = self.management.ip.packed
        return [
            f"02:ca:fe:f0:0d:{hex(t).lstrip('0x').rjust(2, '0')}",
            f"54:52:fe:f0:0d:{hex(m).lstrip('0x').rjust(2, '0')}",
        ]

    def ci_meta_data(self) -> Dict:
        meta_data = dict()
        return meta_data

    def ci_network_config(self) -> Dict:
        ethernets = []
        # yes the nic names are hardcoded...
        def ethernet_config(interface: IPv4Interface):
            return dict(
                addresses=[f"{interface.ip}"],
                gateway4=str(next(interface.network.hosts())),
                dhcp4=False,
                dhcp6=False,
            )

        ens3 = ethernet_config(self.tantap)
        ens4 = ethernet_config(self.management)

        network_config = dict(version=2, ethernets=dict(ens3=ens3, ens4=ens4))
        return network_config

    def ci_user_data(self) -> Dict:
        bootcmd = ["----> START OF TANTAP CLOUD INIT <----------------"]
        # we generate all the possible mapping for the different network
        def _mapping(interface: IPv4Interface, prefix: str):
            host_entries = []
            for _ip in interface.network.hosts():
                _, _, _, d = _ip.exploded.split(".")
                # 192.168.0.123   m123
                host_entries.append((_ip, f"{prefix}{d}"))
            return host_entries

        t_entries = [
            f'echo "{ip}    {alias}" >> /etc/hosts'
            for ip, alias in _mapping(self.tantap, "t")
        ]
        m_entries = [
            f'echo "{ip}    {alias}" >> /etc/hosts'
            for ip, alias in _mapping(self.management, "m")
        ]
        bootcmd.extend(t_entries)
        bootcmd.extend(m_entries)
        bootcmd.append(f'echo "127.0.0.1 {self.hostname}" >> /etc/hosts')

        # - echo 127.0.0.1 $VM_NAME >> /etc/hosts

        user_data = dict(hostname=self.hostname, disable_root=False, bootcmd=bootcmd)
        # non python compatible key
        user_data.update({"local-hostname": self.hostname})
        return user_data

    def prepare_cloud_init(self, working_dir: Path) -> Path:
        # generate cloud init files
        with open(working_dir / "user-data", "w") as f:
            # yes, this is mandatory to prepend this at the beginning of this
            # file
            f.write("#cloud-config\n")
        with open(working_dir / "user-data", "a") as f:
            yaml.dump(self.ci_user_data(), f)
        with open(working_dir / "meta-data", "w") as f:
            yaml.dump(self.ci_meta_data(), f)
        with open(working_dir / "network-config", "w") as f:
            yaml.dump(self.ci_network_config(), f)

        # generate the iso
        iso = (working_dir / "cloud-init.iso").resolve()
        check_call(
            f"genisoimage -output {iso} -volid cidata -joliet -rock user-data meta-data network-config",
            shell=True,
            cwd=working_dir,
        )
        return iso

    def prepare_image(self, working_dir: Path) -> Path:
        qemu_image = (working_dir / "image.qcow2").resolve()
        check_call(
            f"qemu-img create -f qcow2 -o backing_file={self.qemu_image} {qemu_image}",
            shell=True,
        )
        return qemu_image

    def start(self, working_dir: Path) -> Path:
        working_dir.mkdir(parents=True, exist_ok=True)
        # generate cloud_init
        iso = self.prepare_cloud_init(working_dir)
        # generate the base image
        image = self.prepare_image(working_dir)
        # boot
        cmd = (
            f"{self.qemu_cmd}"
            f" {self.qemu_args}"
            f" -drive file={image} "
            f" -cdrom {iso}"
            f" -netdev tantap,src={self.tantap.ip},id=mynet0,ifname={self.tapname[0]},script=no,downscript=no"
            f" -device e1000,netdev=mynet0,mac={self.mac[0]}"
            f" -netdev tap,id=mynet1,ifname={self.tapname[1]},script=no,downscript=no"
            f" -device e1000,netdev=mynet1,mac={self.mac[1]}"
        )
        LOGGER.info(cmd)
        check_call(
            cmd,
            shell=True,
        )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="""
Boots a VM using qemu, icount mode ...

Use two nics: one for tantap the other for a regular tap (management interface).

Assumptions:
    IP addresses are part of a /24 network whose gateway is set to the first possible IP
    ip: 192.168.0.42/24 -> gateway: 192.168.0.1
    So the bridge IP must/will be set accordingly

Some environment Variables (MANDATORY):
    QEMU: path to the qemu binary (useful to test a modified version)
    IMAGE: path to a qcow2 or raw image disk (to serve as backing file for the disk images)

NOTE to self:
sudo ip link add name tantap-br type bridge
sudo ip link set tantap-br up
for tap in {tantap10,tantap11}
do
    sudo ip tuntap add $tap mode tap user msimonin && sudo ip link set $tap master tantap-br && sudo ip link set $tap up
done
sudo ip link add name mantap-br type bridge
sudo ip link set mantap-br up
for tap in {mantap10,mantap11}
do
    sudo ip tuntap add $tap mode tap user msimonin && sudo ip link set $tap master mantap-br && sudo ip link set $tap up
done
""",
        formatter_class=argparse.RawTextHelpFormatter,
    )
    parser.add_argument(
        "ip_tantap",
        type=str,
        help="The ip (in cidr) to use for the tantap interface",
    )

    parser.add_argument(
        "ip_management",
        type=str,
        help="The ip (in cidr) to use for the management interface",
    )

    logging.basicConfig(level=logging.DEBUG)

    args = parser.parse_args()
    ip_tantap = IPv4Interface(args.ip_tantap)
    ip_management = IPv4Interface(args.ip_management)

    # get the mandatory variables from the env
    qemu_cmd = from_env(ENV_QEMU)
    qemu_image = Path(from_env(ENV_IMAGE))

    vm = TansivVM(ip_tantap, ip_management, qemu_cmd=qemu_cmd, qemu_image=qemu_image)

    with tempfile.TemporaryDirectory() as tmp:
        vm.start(working_dir=Path(tmp))