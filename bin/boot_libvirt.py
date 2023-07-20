#!/usr/bin/env python3
import __common
import argparse
from ipaddress import IPv4Interface
import logging
from pathlib import Path
import os
from subprocess import check_call
import tempfile
from typing import Dict, List, Optional
import yaml
from jinja2 import Template, Environment, FileSystemLoader, select_autoescape

LOGGER = logging.getLogger(__name__)

DEFAULT_BASE_WORKING_DIR = Path.cwd() / "tansiv-working-dir"

class VMLibvirt(object):
    def __init__(
        self,
        socket_name: str,
        ip_tantap: IPv4Interface,
        ip_management: IPv4Interface,
        qemu_cmd: str,
        qemu_image: Path,
        descriptor : int,
        qemu_vcpus : str,
        qemu_cpuset : str,
        num_buffers : int,
        qemu_args: str = "",
        hostname: Optional[str] = None,
        public_key: Optional[str] = None,
        autoconfig_net: bool = False,
        qemu_mem: str = "1g",
        qemu_nictype: str = "virtio-net-pci",
        virtio_net_nb_queues: int = 1,
    ):
        self.tantap = ip_tantap
        self.management = ip_management

        self.qemu_cmd = qemu_cmd
        self.qemu_image = qemu_image.resolve()
        self._hostname = hostname
        self.public_key = public_key
        self.autoconfig_net = autoconfig_net

        self.__qemu_args = qemu_args
        self.__mem = qemu_mem
        self.qemu_mem = qemu_mem

        self.__qemu_nictype = qemu_nictype
        self.__virtio_net_nb_queues = virtio_net_nb_queues

        self.descriptor = descriptor
        self.qemu_vcpus = qemu_vcpus
        self.qemu_cpuset = qemu_cpuset

        self.socket_name = socket_name
        self.num_buffers = num_buffers

    @property
    def qemu_args(self):
        return f"{self.__qemu_args} -m {self.__mem}"

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
    def taptype(self) -> List[str]:
        return ["tap", "tap"]

    @property
    def gateway(self) -> List[IPv4Interface]:
        t_cidr = str(self.tantap).split("/")[1]
        m_cidr = str(self.management).split("/")[1]
        return [
            IPv4Interface(f"{str(next(self.tantap.network.hosts()))}/{t_cidr}"),
            IPv4Interface(f"{str(next(self.management.network.hosts()))}/{m_cidr}"),
        ]

    @property
    def nictype(self) -> List[str]:
        return [f"{self.__qemu_nictype}", "virtio-net-pci"]

    @property
    def virtio_net_nb_queues(self) -> List[int]:
        return [self.__virtio_net_nb_queues, 1]

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
        def ethernet_config(mac: str, interface: IPv4Interface, idx: int):
            c = dict(
                match=dict(macaddress=mac),
                # address in cidr
                addresses=[str(interface)],
                gateway4=str(next(interface.network.hosts())),
                # routes=[
                #     dict(to=str(interface), via=str(next(interface.network.hosts())))
                # ],
                dhcp4=False,
                dhcp6=False,
            )
            c.update({"set-name": f"tan{idx}"})
            return c

        mac_tantap, mac_mantap = self.mac
        nic1 = ethernet_config(mac_tantap, self.tantap, 0)
        nic2 = ethernet_config(mac_mantap, self.management, 1)
        network_config = dict(version=2, ethernets=dict(nic1=nic1, nic2=nic2))
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
            f"qemu-img create -f qcow2 -F qcow2 -o backing_file={self.qemu_image} {qemu_image}",
            shell=True,
        )
        return qemu_image

    def _br_tap_cmd(self, br: str, ip: IPv4Interface, tap: str, queues: int):
        """Create a bridge and a tap attached.

        This assumes that the current process is running with the right
        level of privilege.
        """
        if queues == 1:
            extra_tap_opts = ""
        else:
            extra_tap_opts = "vnet_hdr multi_queue"
        return f"""
                    ip link show dev {br} || ip link add name {br} type bridge
                    ip link set {br} up
                    (ip addr show dev {br} | grep {ip}) || ip addr add {ip} dev {br}
                    ip link show dev {tap} || ip tuntap add {tap} mode tap {extra_tap_opts}
                    ip link set {tap} master {br}
                    ip link set {tap} up
                    """

    def prepare_net(self):
        """Create the bridges, the tap if needed."""

        if not self.autoconfig_net:
            return

        prepare_net_cmds = self.prepare_net_cmds()
        for prepare_net_cmd in prepare_net_cmds:
            check_call(prepare_net_cmd, shell=True)

    def prepare_net_cmds(self):
        return [
            self._br_tap_cmd(
                self.bridgename[0],
                self.gateway[0],
                self.tapname[0],
                self.virtio_net_nb_queues[0],
            ),
            self._br_tap_cmd(
                self.bridgename[1],
                self.gateway[1],
                self.tapname[1],
                self.virtio_net_nb_queues[1],
            ),
        ]

    def start(self, working_dir: Path) -> Path:
        # create the working dir
        #   fail if it already exist so that the user can explicitly choose to
        #   remove it (and backup the previous one if needed)
        working_dir.mkdir(parents=True, exist_ok=False)
        # generate cloud_init
        iso = self.prepare_cloud_init(working_dir)
        # generate the base image
        image = self.prepare_image(working_dir)

        self.prepare_net()

        fill_template(
            out=working_dir / f"domain-{self.descriptor}.xml",
            descriptor=self.descriptor,
            qemu_gdb_port =1234+self.descriptor,
            qemu_mem=self.qemu_mem,
            qemu_cmd=self.qemu_cmd,
            qemu_img=image,
            qemu_vcpus=self.qemu_vcpus,
            qemu_cpuset=self.qemu_cpuset,
            qemu_cdrom=iso,
            qemu_tantap_mac=self.mac[0],
            qemu_mantap_mac=self.mac[1],
            qemu_num_buffers=self.num_buffers,
            qemu_tantap_ip=self.tantap.ip,
            qemu_socket_name=self.socket_name,
            qemu_mantap_name=self.tapname[1],
            qemu_tantap_name=self.tapname[0]
        )

        cmd = f"virsh create domain-{self.descriptor}.xml"
        stdout = (working_dir / "out").open("w")
        LOGGER.info(cmd)
        check_call(cmd, shell=True, stdout=stdout, cwd=working_dir)


def terminate(*args):
    import sys

    sys.exit()


def fill_template(
    out: Path, template: str = "templates/deployment-qemukvm-libvirt.xml.j2", **kwargs
):
    env = Environment(
        loader=FileSystemLoader(Path(__file__).parent / "templates"),
        autoescape=select_autoescape(),
    )
    template = env.get_template("deployment-qemukvm-libvirt.xml.j2")
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(
        template.render(**kwargs)
    )


def main():
    import signal

    signal.signal(signal.SIGINT, terminate)
    signal.signal(signal.SIGTERM, terminate)

    parser = argparse.ArgumentParser(
        description="""
Boots a VM using qemu

Use two nics: one for tantap the other for a regular tap (management interface).
The tantap can be a regular tap (for baseline experiment).

Assumptions:
    IP addresses are part of a /24 network whose gateway is set to the first possible IP
    ip: 192.168.0.42/24 -> gateway: 192.168.0.1
    So the bridge IP must/will be set accordingly

Some environment Variables (MANDATORY):
    QEMU: path to the qemu binary (useful to test a modified version)
    IMAGE: path to a qcow2 or raw image disk (to serve as backing file for the disk images)
""",
        formatter_class=argparse.RawTextHelpFormatter,
    )

    parser.add_argument(
        "socket_name",
        type=str,
        help="The (unix) socket name to use for commicating with Tansiv",
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
    parser.add_argument("--mode", type=str, help="mode (tap | tantap)", default="tap")
    parser.add_argument(
        "--qemu_cmd",
        type=str,
        help="qemu cmd to pass",
        default=os.environ.get("QEMU", None),
    )

    parser.add_argument(
        "--qemu_mem",
        type=str,
        help="Memory to use (e.g 1g)",
        default=os.environ.get("QEMU_MEM", "1g"),
    )

    parser.add_argument(
        "--qemu_args",
        type=str,
        help="extra qemu args to pass (e.g.'--icount shift=1,sleep=on,align=off')",
        default=os.environ.get("QEMU_ARGS", ""),
    )

    parser.add_argument(
        "--autoconfig_net",
        action="store_true",
        help="True iff network must be autoconfigured",
        default=os.environ.get("AUTOCONFIG_NET", False),
    )

    parser.add_argument(
        "--qemu_image",
        type=str,
        help="disk image",
        default=os.environ.get("IMAGE", None),
    )

    parser.add_argument(
        "--qemu_nictype",
        type=str,
        help="Model of the main vNIC (e.g. 'virtio-net-pci', 'e1000'). Default is virtio-net-pci.",
        default="virtio-net-pci",
    )

    parser.add_argument(
        "--virtio_net_nb_queues",
        type=int,
        help="""Number of queues for the virtio vNIC. Should be a power of 2
                lower or equal to the number of vCPUs. Default is 1 (no multi-queue).""",
    )

    parser.add_argument(
        "--base_working_dir",
        type=str,
        help="base directory where the working dir will be stored",
    )

    parser.add_argument(
        "--num_buffers",
        type=int,
        help="""Size of the buffer pool of tansiv. This should be set accordingly
to the latency x bandwidth. Undersized buffer pool lead to packet dropping (silently).
The default value is too low for realistics benchmarks.""",
    )

    parser.add_argument(
        "--descriptor",
        type=int,
        help="""Unique integer identifier for the VM""",
    )

    parser.add_argument(
        "--qemu_vcpus",
        type=int,
        help="""Number of vCPUs for the VM""",
    )

    parser.add_argument(
        "--qemu_cpuset",
        type=str,
        help="""Commas separated list of physical CPUs number where vCPUs can be
             pinned to. Can be a range. Use ^ to exclude a CPU previously added
             in a range.""",
    )

    logging.basicConfig(level=logging.DEBUG)

    args = parser.parse_args()

    socket_name = args.socket_name

    ip_tantap = IPv4Interface(args.ip_tantap)
    ip_management = IPv4Interface(args.ip_management)
    hostname = args.hostname

    qemu_cmd = args.qemu_cmd
    if not qemu_cmd:
        raise ValueError("qemu_cmd must be set")
    qemu_args = args.qemu_args
    qemu_mem = args.qemu_mem
    qemu_image = Path(args.qemu_image)
    if not qemu_image:
        raise ValueError("qemu_image must be set")
    qemu_nictype = args.qemu_nictype
    virtio_net_nb_queues = args.virtio_net_nb_queues
    autoconfig_net = args.autoconfig_net

    num_buffers = args.num_buffers

    descriptor = args.descriptor
    qemu_vcpus = args.qemu_vcpus
    qemu_cpuset = args.qemu_cpuset

    # check the required third party software
    check_call("genisoimage --version", shell=True)
    check_call("qemu-img --version", shell=True)

    # will be pushed to the root authorized_keys (root)
    public_key = (Path().home() / ".ssh" / "id_rsa.pub").open().read()

    vm = VMLibvirt(
        socket_name,
        ip_tantap,
        ip_management,
        descriptor=descriptor,
        qemu_vcpus=qemu_vcpus,
        qemu_cpuset=qemu_cpuset,
        qemu_cmd=qemu_cmd,
        qemu_image=qemu_image,
        qemu_args=qemu_args,
        hostname=hostname,
        public_key=public_key,
        autoconfig_net=autoconfig_net,
        qemu_mem=qemu_mem,
        num_buffers=num_buffers,
        qemu_nictype=qemu_nictype,
        virtio_net_nb_queues=virtio_net_nb_queues,
    )

    # base_working_dir allows to gather in a predefined place all the working dirs.
    base_working_dir = (
        Path(args.base_working_dir)
        if args.base_working_dir
        else DEFAULT_BASE_WORKING_DIR
    )
    # create it if it doesn't exist
    Path(base_working_dir).mkdir(exist_ok=True, parents=True)

    # craft the working dir
    # this is where some of the vm specific state / output will be stored

    working_dir = base_working_dir / vm.hostname    

    LOGGER.info(f"Launching in {working_dir}")
    vm.start(working_dir=working_dir)


if __name__ == "__main__":
    main()