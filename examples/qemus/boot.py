#!/usr/bin/env python3
import argparse
from ipaddress import IPv4Interface
import logging
from pathlib import Path
import os
from subprocess import check_call
import tempfile
from typing import Any, Dict, List, Optional
import yaml

# some env variable supported by the program
ENV_QEMU = "QEMU"
ENV_IMAGE = "IMAGE"
ENV_AUTOCONFIG_NET = "AUTOCONFIG_NET"

LOGGER = logging.getLogger(__name__)


def from_env(key: str, default: Optional[Any] = None) -> str:
    value = os.environ.get(key, default)
    if value is None:
        raise ValueError(f"Missing {key} in the environment")
    return value


class TansivVM(object):
    """
    TODO autogenerate this based on the cli
    """

    def __init__(
        self,
        ip_tantap: IPv4Interface,
        ip_management: IPv4Interface,
        qemu_cmd: str,
        qemu_image: Path(),
        qemu_args: Optional[str] = None,
        hostname: Optional[str] = None,
        public_key: Optional[str] = None,
        autoconfig_net: bool = False,
    ):
        self.tantap = ip_tantap
        self.management = ip_management
        self.qemu_cmd = qemu_cmd
        self.qemu_image = qemu_image.resolve()
        self._hostname = hostname
        self.public_key = public_key
        self.autoconfig_net = autoconfig_net

        if qemu_args is None:
            # Tansiv profile ?
            self.qemu_args = (
                " --icount shift=1,sleep=on"
                " -rtc clock=vm"
                f" -m 1g --vsg mynet0,src={ip_tantap.ip}"
            )
        else:
            self.qemu_args = qemu_args

    @property
    def tantap_id(self):
        _, _, _, t = self.tantap.ip.packed
        return t

    @property
    def management_id(self):
        _, _, _, m = self.management.ip.packed
        return m

    @property
    def hostname(self) -> str:
        if self._hostname is None:
            return f"tansiv-{str(self.tantap.ip).replace('.', '-')}"
        return self._hostname

    @property
    def tapname(self) -> List[str]:
        t = self.tantap_id
        m = self.management_id
        return [f"tantap{t}", f"mantap{m}"]

    @property
    def bridgename(self) -> List[str]:
        return ["tantap-br", "mantap-br"]

    @property
    def gateway(self) -> List[IPv4Interface]:
        t_cidr = str(self.tantap).split("/")[1]
        m_cidr = str(self.management).split("/")[1]
        return [
            IPv4Interface(f"{str(next(self.tantap.network.hosts()))}/{t_cidr}"),
            IPv4Interface(f"{str(next(self.management.network.hosts()))}/{m_cidr}"),
        ]

    @property
    def mac(self) -> List[str]:
        t = self.tantap_id
        m = self.management_id
        return [
            f"02:ca:fe:f0:0d:{hex(t).lstrip('0x').rjust(2, '0')}",
            f"54:52:fe:f0:0d:{hex(m).lstrip('0x').rjust(2, '0')}",
        ]

    def ci_meta_data(self) -> Dict:
        meta_data = dict()
        meta_data.update({"instance-id": self.hostname})
        if self.public_key is not None:
            meta_data.update({"public-keys": self.public_key})
        return meta_data

    def ci_network_config(self) -> Dict:
        # yes the nic names are hardcoded...
        def ethernet_config(interface: IPv4Interface):
            return dict(
                addresses=[f"{interface.ip}"],
                gateway4=str(next(interface.network.hosts())),
                # routes=[
                #     dict(to=str(interface), via=str(next(interface.network.hosts())))
                # ],
                dhcp4=False,
                dhcp6=False,
            )

        ens3 = ethernet_config(self.tantap)
        ens4 = ethernet_config(
            self.management,
        )
        network_config = dict(version=2, ethernets=dict(ens3=ens3, ens4=ens4))
        LOGGER.debug(network_config)
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
            for ip, alias in _mapping(self.tantap, "tantap")
        ]
        m_entries = [
            f'echo "{ip}    {alias}" >> /etc/hosts'
            for ip, alias in _mapping(self.management, "mantap")
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

    def prepare_net(self):
        """Create the bridges, the tap if needed."""

        def br_tap(br: str, ip: IPv4Interface, tap: str):
            """Create a bridge and a tap attached.

            This assumes that the current process is running with the right
            level of privilege.
            """
            check_call(
                f"""
                       ip link show dev {br} || ip link add name {br} type bridge
                       ip link set {br} up
                       (ip addr show dev {br} | grep {ip}) || ip addr add {ip} dev {br}
                       ip link show dev {tap} || ip tuntap add {tap} mode tap
                       ip link set {tap} master {br}
                       ip link set {tap} up
                       """,
                shell=True,
            )

        if not self.autoconfig_net:
            return

        br_tap(self.bridgename[0], self.gateway[0], self.tapname[0])
        br_tap(
            self.bridgename[1],
            self.gateway[1],
            self.tapname[1],
        )

    def start(self, working_dir: Path) -> Path:
        working_dir.mkdir(parents=True, exist_ok=True)
        # generate cloud_init
        iso = self.prepare_cloud_init(working_dir)
        # generate the base image
        image = self.prepare_image(working_dir)

        self.prepare_net()
        # boot
        cmd = (
            f"{self.qemu_cmd}"
            f" {self.qemu_args}"
            f" -drive file={image} "
            f" -cdrom {iso}"
            f" -netdev tantap,id=mynet0,ifname={self.tapname[0]},script=no,downscript=no"
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
    parser.add_argument(
        "--hostname",
        type=str,
        help="The hostname of the virtual machine",
    )

    parser.add_argument("--qemu-args", type=str, help="arguments to pass to qemu")

    logging.basicConfig(level=logging.DEBUG)

    args = parser.parse_args()
    ip_tantap = IPv4Interface(args.ip_tantap)
    ip_management = IPv4Interface(args.ip_management)
    qemu_args = args.qemu_args
    hostname = args.hostname

    # get the mandatory variables from the env
    qemu_cmd = from_env(ENV_QEMU)
    qemu_image = Path(from_env(ENV_IMAGE))
    # check the required third party software
    check_call("genisoimage --help", shell=True)
    check_call("qemu-img --help", shell=True)
    autoconfig_net = from_env(ENV_AUTOCONFIG_NET, False)

    # will be pushed to the root authorized_keys (root)
    public_key = (Path().home() / ".ssh" / "id_rsa.pub").open().read()

    vm = TansivVM(
        ip_tantap,
        ip_management,
        qemu_cmd=qemu_cmd,
        qemu_image=qemu_image,
        qemu_args=qemu_args,
        hostname=hostname,
        public_key=public_key,
        autoconfig_net=autoconfig_net,
    )

    with tempfile.TemporaryDirectory() as tmp:
        LOGGER.info(f"Launching in {tmp}")
        vm.start(working_dir=Path(tmp))