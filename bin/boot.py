#!/usr/bin/env python3
from abc import abstractmethod
import argparse
from ipaddress import IPv4Interface
from jinja2 import Environment, FileSystemLoader, select_autoescape
import logging
import os
from pathlib import Path
from subprocess import check_call, check_output, Popen
from typing import Dict, List, Optional
import yaml

LOGGER = logging.getLogger(__name__)

DEFAULT_BASE_WORKING_DIR = Path.cwd() / "tansiv-working-dir"


def _fill_template(template, out, **kwargs):
    env = Environment(
        loader=FileSystemLoader(
            Path(__file__).parent / "templates"
        ),  # TODO: Put the templates elsewhere than /bin
        autoescape=select_autoescape(),
    )
    template = env.get_template(template)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(template.render(**kwargs))


class VM(object):
    def __init__(
        self,
        descriptor: int,
        ip_tantap: IPv4Interface,
        ip_management: IPv4Interface,
        qemu_cmd: str,
        image: Path,
        qemu_args: str = "",
        hostname: Optional[str] = None,
        public_key: Optional[str] = None,
        autoconfig_net: bool = False,
        mem: str = "1g",
        qemu_nictype: str = "virtio-net-pci",
        virtio_net_nb_queues: int = 1,
        cores: Optional[int] = None,
        mac_address: Optional[str] = None,
    ):
        self.descriptor = descriptor
        self.tantap = ip_tantap
        self.management = ip_management

        self.qemu_cmd = qemu_cmd
        # force image to be a Path
        image = Path(image)
        self.image = image.resolve()
        self._hostname = hostname
        self.public_key = public_key
        self.autoconfig_net = autoconfig_net

        self.__qemu_args = qemu_args
        self.mem = mem

        self.__qemu_nictype = qemu_nictype
        self.__virtio_net_nb_queues = virtio_net_nb_queues

        self.cores = 1 if cores is None else cores

        self.__mac_address = mac_address

    @property
    def qemu_args(self):
        return f"{self.__qemu_args} -m {self.mem}"

    @property
    def tantap_id(self):
        return self.descriptor

    @property
    def management_id(self):
        return self.descriptor

    @property
    def hostname(self) -> str:
        if self._hostname is None:
            return f"tansiv-{self.descriptor}"
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
        tantap_mac = (
            f"02:ca:fe:f0:0d:{hex(t).lstrip('0x').rjust(2, '0')}"
            if self.__mac_address is None
            else self.__mac_address
        )
        return [
            tantap_mac,
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
        image = (working_dir / "image.qcow2").resolve()
        check_call(
            f"qemu-img create -f qcow2 -F qcow2 -o backing_file={self.image} {image}",
            shell=True,
        )
        return image

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

    def prepare_start(self, working_dir: Path) -> tuple[Path, Path]:
        # create the working dir
        #   fail if it already exist so that the user can explicitly choose to
        #   remove it (and backup the previous one if needed)
        working_dir.mkdir(parents=True, exist_ok=False)
        # generate cloud_init
        iso = self.prepare_cloud_init(working_dir)
        # generate the base image
        image = self.prepare_image(working_dir)

        # abstract
        self.prepare_net()

        return iso, image

    @abstractmethod
    def start(self, working_dir: Path) -> None: ...


class TansivQemu(VM):
    def __init__(
        self, socket_name: str, *args, num_buffers: Optional[int] = None, **kwargs
    ):
        self.socket_name = socket_name
        self.num_buffers = num_buffers
        super().__init__(*args, **kwargs)

    @property
    def qemu_args(self):
        qemu_args = super().qemu_args
        cmd = (
            qemu_args
            + " "
            + f"--vsg mynet0,socket={self.socket_name},src={self.tantap.ip}"
        )

        if self.num_buffers:
            cmd += f",num_buffers={self.num_buffers}"
        return cmd

    @property
    def taptype(self) -> List[str]:
        return ["tantap", "tap"]

    def prepare_cmd(self, image: Path, iso: Path) -> str:
        """
        python -m doctest boot.py
        >>> tansiv_qemu = TansivQemu(
        ...                "socket",
        ...                1,
        ...                IPv4Interface("192.168.120.1/24"),
        ...                IPv4Interface("10.0.0.1/24"),
        ...                "qemu-system-x86_64",
        ...                Path("image.qcow2"),
        ...                num_buffers=42)
        >>> tansiv_qemu.prepare_cmd(Path("image_copy.qcow2"), Path("cloud_init.iso"))
        'qemu-system-x86_64  -m 1g --vsg mynet0,socket=socket,src=192.168.120.1,num_buffers=42 -drive file=image_copy.qcow2  -cdrom cloud_init.iso -netdev tantap,id=mynet0,ifname=tantap1,script=no,downscript=no -device virtio-net-pci,netdev=mynet0,mac=02:ca:fe:f0:0d:01 -netdev tap,id=mynet1,ifname=mantap1,script=no,downscript=no -device virtio-net-pci,netdev=mynet1,mac=54:52:fe:f0:0d:01 -gdb tcp::1235,server,nowait'

        >>> tansiv_qemu = TansivQemuICount(
        ...                "socket",
        ...                1,
        ...                IPv4Interface("192.168.120.1/24"),
        ...                IPv4Interface("10.0.0.1/24"),
        ...                "qemu-system-x86_64",
        ...                Path("image.qcow2"),
        ...                num_buffers=42)
        >>> tansiv_qemu.prepare_cmd(Path("image_copy.qcow2"), Path("cloud_init.iso"))
        'qemu-system-x86_64  -m 1g --vsg mynet0,socket=socket,src=192.168.120.1,num_buffers=42 -icount shift=0,sleep=off,align=off -rtc clock=vm -drive file=image_copy.qcow2  -cdrom cloud_init.iso -netdev tantap,id=mynet0,ifname=tantap1,script=no,downscript=no -device virtio-net-pci,netdev=mynet0,mac=02:ca:fe:f0:0d:01 -netdev tap,id=mynet1,ifname=mantap1,script=no,downscript=no -device virtio-net-pci,netdev=mynet1,mac=54:52:fe:f0:0d:01 -gdb tcp::1235,server,nowait'



        >>> tansiv_qemu = TansivQemuKVM(
        ...                "socket",
        ...                1,
        ...                IPv4Interface("192.168.120.1/24"),
        ...                IPv4Interface("10.0.0.1/24"),
        ...                "qemu-system-x86_64",
        ...                Path("image.qcow2"),
        ...                num_buffers=42)
        >>> tansiv_qemu.prepare_cmd(Path("image_copy.qcow2"), Path("cloud_init.iso"))
        'qemu-system-x86_64  -m 1g --vsg mynet0,socket=socket,src=192.168.120.1,num_buffers=42 -accel kvm -smp sockets=1,cores=1,threads=1,maxcpus=1 -monitor unix:/srv/tansiv/qemu-monitor-1,server,nowait -cpu max,invtsc=on -drive file=image_copy.qcow2  -cdrom cloud_init.iso -netdev tantap,id=mynet0,ifname=tantap1,script=no,downscript=no -device virtio-net-pci,netdev=mynet0,mac=02:ca:fe:f0:0d:01 -netdev tap,id=mynet1,ifname=mantap1,script=no,downscript=no -device virtio-net-pci,netdev=mynet1,mac=54:52:fe:f0:0d:01 -gdb tcp::1235,server,nowait'

        """
        if self.virtio_net_nb_queues[0] == 1:
            multi_queues_opt_tap = ""
            multi_queues_opt_virtio = ""
        else:
            multi_queues_opt_tap = f",queues={self.virtio_net_nb_queues[0]},vhost=off"
            multi_queues_opt_virtio = f",mq=on,vectors={str(2 * self.virtio_net_nb_queues[0] + 2)},rss=on,hash=on"

        # The following code can be removed if we do not want to support multi queue for the management interface
        if self.virtio_net_nb_queues[1] == 1:
            multi_queues_opt_tap_management = ""
            multi_queues_opt_virtio_management = ""
        else:
            multi_queues_opt_tap_management = (
                f",queues={self.virtio_net_nb_queues[1]},vhost=off"
            )
            multi_queues_opt_virtio_management = f",mq=on,vectors={str(2 * self.virtio_net_nb_queues[1] + 2)},rss=on,hash=on"

        cmd = (
            f"{self.qemu_cmd}"
            f" {self.qemu_args}"
            f" -drive file={image} "
            f" -cdrom {iso}"
            f" -netdev {self.taptype[0]},id=mynet0,ifname={self.tapname[0]},script=no,downscript=no{multi_queues_opt_tap}"
            f" -device {self.nictype[0]},netdev=mynet0,mac={self.mac[0]}{multi_queues_opt_virtio}"
            f" -netdev {self.taptype[1]},id=mynet1,ifname={self.tapname[1]},script=no,downscript=no{multi_queues_opt_tap_management}"
            f" -device {self.nictype[1]},netdev=mynet1,mac={self.mac[1]}{multi_queues_opt_virtio_management}"
            f" -gdb tcp::{1234 + self.management_id},server,nowait"
        )

        return cmd

    def start(self, working_dir: Path):
        iso, image = self.prepare_start(working_dir)

        cmd = self.prepare_cmd(image, iso)

        stdout = (working_dir / "out").open("w")
        LOGGER.info(cmd)
        check_call(cmd, shell=True, stdout=stdout, cwd=working_dir)


class TansivQemuICount(TansivQemu):
    @property
    def qemu_args(self):
        qemu_args = super().qemu_args
        cmd = f"{qemu_args} -icount shift=0,sleep=off,align=off -rtc clock=vm"
        return cmd


class TansivQemuKVM(TansivQemu):
    @property
    def qemu_args(self):
        qemu_args = super().qemu_args
        cmd = (
            f"{qemu_args}"
            f" -accel kvm -smp sockets=1,cores={self.cores},threads=1,maxcpus={self.cores}"
            f" -monitor unix:/srv/tansiv/qemu-monitor-{self.descriptor},server,nowait"
            f" -cpu max,invtsc=on"
            f" -overcommit cpu-pm=on"
        )
        return cmd


class TansivLibvirt(VM):
    def __init__(
        self,
        socket_name: str,
        *args,
        template: str = "deployment-qemukvm-libvirt.xml.j2",
        cpuset: str = "",
        num_buffers: Optional[int] = None,
        **kwargs,
    ):
        self.socket_name = socket_name
        self.cpuset = cpuset
        self.num_buffers = num_buffers
        if not self.cpuset:
            if not self.cores:
                self.cpuset = "0"
            else:
                self.cpuset = f"0-{self.cores}"
        self.template = template
        super().__init__(*args, **kwargs)

    def start(self, working_dir: Path):
        iso, image = self.prepare_start(working_dir)

        _fill_template(
            self.template,
            out=working_dir / f"domain-{self.descriptor}.xml",
            descriptor=self.descriptor,
            qemu_gdb_port=1234 + self.descriptor,
            mem=self.mem,
            qemu_cmd=self.qemu_cmd,
            qemu_img=image,
            qemu_vcpus=self.cores,
            qemu_cpuset=self.cpuset,
            qemu_cdrom=iso,
            qemu_tantap_mac=self.mac[0],
            qemu_mantap_mac=self.mac[1],
            qemu_num_buffers=self.num_buffers,
            qemu_tantap_ip=self.tantap.ip,
            qemu_socket_name=self.socket_name,
            qemu_mantap_name=self.tapname[1],
            qemu_tantap_name=self.tapname[0],
        )

        cmd = f"virsh create domain-{self.descriptor}.xml"
        stdout = (working_dir / "out").open("w")
        LOGGER.info(cmd)
        check_call(cmd, shell=True, stdout=stdout, cwd=working_dir)


class TansivLibvirtVMI(TansivLibvirt):
    def __init__(self, *args, **kwargs):
        super().__init__(
            *args, template="deployment-qemukvmvmi-libvirt.xml.j2", **kwargs
        )


class TansivXen(VM):
    def __init__(
        self,
        socket_name: str,
        *args,
        template: str = "deployment-xen.cfg.j2",
        cpuset: str = "",
        num_buffers: Optional[int] = None,
        **kwargs,
    ):
        self.socket_name = socket_name
        self.cpuset = cpuset
        self.num_buffers = num_buffers
        if not self.cpuset:
            if not self.cores:
                self.cpuset = "0"
            else:
                self.cpuset = f"0-{self.cores}"
        self.template = template
        super().__init__(*args, **kwargs)

    def start(self, working_dir: Path):
        iso, image = self.prepare_start(working_dir)

        _fill_template(
            self.template,
            out=working_dir / f"domain-{self.descriptor}.cfg",
            descriptor=self.descriptor,
            xen_mem=self.mem,
            xen_img=image,
            xen_vcpus=self.cores,
            xen_cpuset=self.cpuset,
            xen_cdrom=iso,
            xen_tantap_mac=self.mac[0],
            xen_mantap_mac=self.mac[1],
            xen_tantap_bridge=self.bridgename[0],
            xen_mantap_bridge=self.bridgename[1],
        )

        cmd_xl = f"xl create -f domain-{self.descriptor}.cfg"
        with (working_dir / "out").open("w") as stdout:
            check_call(cmd_xl, shell=True, stdout=stdout, cwd=working_dir)

        # get the domid
        domid = check_output(f"xl domid tansiv-{self.descriptor}", shell=True, cwd=working_dir).decode().strip()
        with (working_dir / "out").open("w") as stdout:
            Popen(["/opt/tansiv/bin/xen_tansiv_bridge", f"tansiv-{self.descriptor}", f"{self.socket_name}", f"{self.tantap.ip}", f"{self.num_buffers}", f"{domid}", f"vif{domid}.0"], stdout=stdout, cwd=working_dir)


def terminate(*args):
    import sys

    sys.exit()


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
    # The coordinator requires socket_name as the the first argument
    parser.add_argument(
        "socket_name",
        type=str,
        help="The (unix) socket name to use for commicating with Tansiv.",
    )

    parser.add_argument(
        "mode",
        choices=["icount", "kvm", "libvirt", "libvirt_vmi", "xen"],
        help="mode ...",
    )

    parser.add_argument(
        "ip_tantap",
        type=str,
        help="The ip (in cidr) to use for the tantap interface.",
    )

    parser.add_argument(
        "ip_management",
        type=str,
        help="The ip (in cidr) to use for the management interface.",
    )

    parser.add_argument(
        "descriptor", type=int, help="Unique integer ID to identify the VM."
    )

    parser.add_argument(
        "--hostname",
        type=str,
        help="The hostname of the virtual machine. Default is 'tansiv-{descriptor}'.",
    )

    parser.add_argument(
        "--qemu_cmd",
        type=str,
        help="qemu cmd to pass. Default is QEMU.",
        default=os.environ.get("QEMU", None),
    )

    parser.add_argument(
        "--mem",
        type=str,
        help="""Memory to use. For QEMU/KVM you can use "M" or "G" as a suffix
             to signify a value in megabytes/gigabytes. For Xen the value must
             be an integer, for which the unit will be megabytes.  Default is QEMU_MEM, or if not defined 1GB.""",
        default=os.environ.get("QEMU_MEM", None),
    )

    parser.add_argument(
        "--qemu_args",
        type=str,
        help="extra qemu args to pass. Default is QEMU_ARGS.",
        default=os.environ.get("QEMU_ARGS", ""),
    )

    parser.add_argument(
        "--autoconfig_net",
        action="store_true",
        help="True iff network must be autoconfigured. Default is AUTOCONFIG_NET, or if not defined False.",
        default=os.environ.get("AUTOCONFIG_NET", False),
    )

    parser.add_argument(
        "--image",
        type=str,
        help="Disk image path. Default is IMAGE.",
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
        default=1,
    )

    parser.add_argument(
        "--base_working_dir",
        type=str,
        help="Base directory where the working dir will be stored. Default is ./tansiv-working-dir .",
    )

    parser.add_argument(
        "--num_buffers",
        type=int,
        help="""Size of the buffer pool of tansiv. This should be set accordingly
to the latency x bandwidth. Undersized buffer pool lead to packet dropping (silently).
The default value is too low for realistics benchmarks.""",
    )

    parser.add_argument(
        "--cores",
        type=int,
        help="Number of cores. Default is 1. Forced to 1 in icount mode",
        default=1,
    )

    parser.add_argument(
        "--cpuset",
        type=str,
        help="""Cores on which the vCPUs must be pinned. Examples: '0', '2-5', '1, ^3, 4-6'.
                Default is '0-{cores}'. Only compatible with libvirt and Xen modes.""",
    )

    parser.add_argument(
        "--mac",
        type=str,
        help="Mac address for the main network interface. Default is derivated from the VM id.",
    )

    logging.basicConfig(level=logging.DEBUG)

    args = parser.parse_args()

    d = {}

    d["socket_name"] = args.socket_name

    d["ip_tantap"] = IPv4Interface(args.ip_tantap)
    d["ip_management"] = IPv4Interface(args.ip_management)
    d["hostname"] = args.hostname

    d["qemu_cmd"] = args.qemu_cmd
    if not args.qemu_cmd and args.mode != "xen":
        raise ValueError("qemu_cmd must be set")
    d["qemu_args"] = args.qemu_args
    if args.mem == None and args.mode == "xen":
        d["mem"] = "1000"
    elif args.mem == None:
        d["mem"] = "1G"
    else:
        d["mem"] = args.mem
    d["image"] = Path(args.image)
    if not args.image:
        raise ValueError("image must be set")
    d["qemu_nictype"] = args.qemu_nictype
    d["virtio_net_nb_queues"] = args.virtio_net_nb_queues
    d["autoconfig_net"] = args.autoconfig_net

    d["num_buffers"] = args.num_buffers

    d["cores"] = args.cores
    if args.mode in ["xen", "libvirt", "libvirt-vmi"]:
        d["cpuset"] = args.cpuset
    d["descriptor"] = args.descriptor
    d["mac_address"] = args.mac

    # check the required third party software
    check_call("genisoimage --version", shell=True)
    check_call("qemu-img --version", shell=True)

    # will be pushed to the root authorized_keys (root)
    public_key = (Path().home() / ".ssh" / "id_rsa.pub").open().read()
    d["public_key"] = public_key

    if args.mode == "icount":
        vm = TansivQemuICount(**d)
    elif args.mode == "kvm":
        vm = TansivQemuKVM(**d)
    elif args.mode == "libvirt":
        vm = TansivLibvirt(**d)
    elif args.mode == "libvirt-vmi":
        vm = TansivLibvirtVMI(**d)
    elif args.mode == "xen":
        vm = TansivXen(**d)
    else:
        raise ValueError("Unknown mode")

    for cmd in vm.prepare_net_cmds():
        print(cmd)

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
