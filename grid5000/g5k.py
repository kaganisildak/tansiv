import argparse
import logging
from ipaddress import IPv4Interface
from pathlib import Path
import traceback

import enoslib as en


DEFAULT_DOCKER_IMAGE = "registry.gitlab.inria.fr/quinson/2018-vsg/tansiv:latest"


class TansivHost(en.Host):
    """
    A TansivHost is an EnOSlib Host but requires an extra jump to access it

    local_machine --ssh1--> g5k machine --ssh2--> tansiv host
    ssh1 also requires a jump through the access machine but that should be
    already set in your .ssh/config
    ssh2 is what we handle here.
    """

    def __init__(self, address, alias_descriptor, g5k_machine):
        super().__init__(address, alias=f"mantap{alias_descriptor}", user="root")
        self.extra.update(
            tansiv_alias=f"tantap{alias_descriptor}",
            gateway=g5k_machine,
            gateway_user="root",
        )


def build_tansiv_roles(deployment: Path, tansiv_node: en.Host) -> en.Roles:
    """Build enoslib roles based on a simgrid deployment file.

    Args:
        deployment: Path to the deployment file
        tansiv_node: the Host representing the node where tansiv is launched

    Returns
        The roles representing the virtual machines launched by tansiv
        according to the deployment file.
    """
    # build the inventory based on the deployment file in use
    import xml.etree.ElementTree as ET

    tree = ET.parse(str(deployment))
    root = tree.getroot()
    ip_ifaces = sorted(
        [
            IPv4Interface(e.attrib["value"])
            for e in root.findall("./actor/argument[last()]")
        ]
    )
    tansiv_roles = dict(
        all=[
            TansivHost(str(ip_iface.ip), ip_iface.ip.packed[-1], tansiv_node.address)
            for ip_iface in ip_ifaces
        ]
    )
    print(tansiv_roles)
    return tansiv_roles


def generate_deployment(args) -> str:
    """Generate a deployment file with size vms.

    Many things remain hardcoded (e.g physical host to map the processes).
    """
    size = int(args.size)
    out = args.out
    import xml.etree.ElementTree as ET
    from xml.etree.ElementTree import ElementTree

    platform = ET.Element("platform", dict(version="4.1"))
    for i in range(1, size + 1):
        # we start at 10.0.0.10/192.168.120.10
        descriptor = 9 + i
        vm = ET.SubElement(
            platform,
            "actor",
            dict(host=f"nova-{i}.lyon.grid5000.fr", function="vsg_vm"),
        )
        ET.SubElement(vm, "argument", dict(value=f"192.168.120.{descriptor}"))
        ET.SubElement(vm, "argument", dict(value="./boot.py"))
        ET.SubElement(vm, "argument", dict(value=f"192.168.120.{descriptor}/24"))
        ET.SubElement(vm, "argument", dict(value=f"10.0.0.{descriptor}/24"))

    element_tree = ElementTree(platform)
    with open(out, "w") as f:
        f.write('<!DOCTYPE platform SYSTEM "https://simgrid.org/simgrid.dtd">')
        f.write(ET.tostring(platform, encoding="unicode"))


def start_tansiv(docker_image: str, p: en.play_on):
    p.docker_container(
        state="started",
        network_mode="host",
        name="tansiv",
        image=docker_image,
        command="platform.xml deployment.xml",
        volumes=["/tmp/id_rsa.pub:/root/.ssh/id_rsa.pub", "/tmp/tansiv:/srv"],
        env={
            "AUTOCONFIG_NET": "true",
            "IMAGE": "image.qcow2",
        },
        capabilities=["NET_ADMIN"],
        devices=["/dev/net/tun"],
    )


@en.enostask(new=True)
def deploy(args, env=None):
    """Deploy tansiv and the associated VMs.

    idempotent.
    """
    image = args.image
    cluster = args.cluster
    platform = args.platform
    deployment = args.deployment
    walltime = args.walltime
    queue = args.queue
    docker_image = args.docker_image
    prod = en.G5kNetworkConf(
        id="id",
        roles=["prod"],
        site=en.g5k_api_utils.get_cluster_site(cluster),
        type="prod",
    )
    conf = (
        en.G5kConf.from_settings(
            job_name="tansiv",
            job_type="allow_classic_ssh",
            walltime=walltime,
            queue=queue,
        )
        .add_machine(cluster=cluster, roles=["tansiv"], nodes=1, primary_network=prod)
        .add_network_conf(prod)
    ).finalize()

    provider = en.G5k(conf)
    roles, _ = provider.init()

    # install docker
    docker = en.Docker(agent=roles["tansiv"], bind_var_docker="/tmp/docker")
    docker.deploy()

    # copy my ssh key
    pub_key = Path.home() / ".ssh" / "id_rsa.pub"
    if not pub_key.exists() or not pub_key.is_file():
        raise Exception(f"No public key found in {pub_key}")

    with en.play_on(roles=roles) as p:
        # copy the pub_key
        p.copy(src=str(pub_key), dest="/tmp/id_rsa.pub")
        # copy also the example/qemu dir
        # assumes that the qcow2 image is there
        p.file(path="/tmp/tansiv", state="directory")
        p.synchronize(
            src=image, dest="/tmp/tansiv/image.qcow2", display_name="copying base image"
        )
        p.synchronize(
            src=platform,
            dest="/tmp/tansiv/platform.xml",
            display_name="copying platform file",
        )
        p.synchronize(
            src=deployment,
            dest="/tmp/tansiv/deployment.xml",
            display_name="copying deployment file",
        )
        # we also need the boot.py wrapper
        p.synchronize(
            src="../examples/qemus/boot.py",
            dest="/tmp/tansiv/boot.py",
            display_name="copying deployment file",
        )
        # finally start the container
        start_tansiv(docker_image, p)
        # by default packets that needs to be forwarded by the bridge are sent to iptables
        # iptables will most likely drop them.
        # we can disabled this behaviour by bypassing iptables
        # https://wiki.libvirt.org/page/Net.bridge.bridge-nf-call_and_sysctl.conf
        # We have two bridges currently
        # - the tantap bridge: only used for traffic not supported by the vsg implementation (e.g arp request, dhcp)
        # - the mantap bridge: used for management tasks, traffic follow a
        #   regular flow through the bridge so might be dropped by iptables (e.g ping from m10 to m11)
        p.sysctl(
            name="net.bridge.bridge-nf-call-iptables",
            value="0",
            state="present",
            sysctl_set="yes",
        )
        p.sysctl(
            name="net.bridge.bridge-nf-call-arptables",
            value="0",
            state="present",
            sysctl_set="yes",
        )

    tansiv_roles = build_tansiv_roles(Path(deployment), roles["tansiv"][0])

    # waiting for the tansiv vms to show up
    en.wait_for(roles=tansiv_roles)
    env["roles"] = roles
    env["tansiv_roles"] = tansiv_roles
    env["provider"] = provider


@en.enostask()
def fping(args, env=None):
    """Validates the deployment.

    Idempotent.
    Only run fping on the remote hosts to get a matrix of latencies.
    """
    tansiv_roles = env["tansiv_roles"]
    # dummy validation
    # -- runs fping and get point to point latency for every pair of nodes
    # -- assuming that mXX is the name of the machine on the management interface
    # -- assuming that tXX is the name of the machien on the tansiv interface
    hostnames = [h.alias for h in tansiv_roles["all"]] + [
        h.extra["tansiv_alias"] for h in tansiv_roles["all"]
    ]
    print(hostnames)
    result = en.run_command(
        f'fping -q -C 10 -s -e {" ".join(hostnames)}',
        roles=tansiv_roles,
        on_error_continue=True,
    )

    # displayng the output (the result datastructure is a bit painful to parse...ask enoslib maintainer)
    for hostname, r in result["ok"].items():
        print(f"################## <{hostname}> #################")
        # fping stats are displayed on stderr
        print(r["stderr"])
        print(f"################## </{hostname}> #################")

    for hostname, r in result["failed"].items():
        print(f"host that fails = {hostname}")


@en.enostask()
def flent(args, env=None):
    """Runs flent."""
    tansiv_roles = env["tansiv_roles"]
    hosts = tansiv_roles["all"]
    # split the hosts
    masters = hosts[0:2:]
    workers = hosts[1:2:]
    for worker in workers:
        worker.extra.update(flent_server=masters[0].extra["tansiv_alias"])

    with en.play_on(roles=dict(all=masters)) as p:
        p.shell(
            "(tmux ls | grep netserver ) ||tmux new-session -s netserver -d 'netserver -D'"
        )

    with en.play_on(roles=dict(all=workers)) as p:
        p.shell("flent udp_flood -p totals -l 10 -H {{ flent_server }} -o filename.png")
        p.fetch(src="filename.png", dest="result")


@en.enostask()
def destroy(args, env=None):
    force = args.force
    if force:
        provider = env["provider"]
        provider.destroy()
    else:
        # be kind / soft removal
        roles = env["roles"]
        with en.play_on(roles=roles) as p:
            p.docker_container(
                name="tansiv", state="absent", display_name="Removing tansiv container"
            )


if __name__ == "__main__":
    logging.basicConfig(level=logging.DEBUG)

    parser = argparse.ArgumentParser(description="Tansiv experimentation engine")
    # FIXME

    # ------------------------------------------------------------------- DEPLOY
    subparsers = parser.add_subparsers(help="deploy")
    parser_deploy = subparsers.add_parser(
        "deploy", help="Deploy tansiv and the associated VMs"
    )
    parser_deploy.add_argument(
        "image",
        help="Base image to use (qcow2)",
    )
    parser_deploy.add_argument(
        "platform",
        help="The simgrid plaform file",
    )
    parser_deploy.add_argument(
        "deployment",
        help="The simgrid deployment file",
    )
    parser_deploy.add_argument(
        "--cluster", help="Cluster where to get the node", default="parapluie"
    )
    parser_deploy.add_argument(
        "--walltime", help="Walltime for the reservation", default="02:00:00"
    )
    parser_deploy.add_argument("--queue", help="Qeueue to use", default="default")
    parser_deploy.add_argument(
        "--docker_image",
        help="Tansiv docker image to use",
        default=DEFAULT_DOCKER_IMAGE,
    )

    parser_deploy.set_defaults(func=deploy)
    # --------------------------------------------------------------------------

    # -------------------------------------------------------------------- FPING
    parser_fping = subparsers.add_parser("fping", help="Run a fping in full mesh")
    parser_fping.set_defaults(func=fping)
    # --------------------------------------------------------------------------

    # -------------------------------------------------------------------- FLENT
    parser_flent = subparsers.add_parser("flent", help="Run flent")
    parser_flent.set_defaults(func=flent)
    parser_flent.add_argument(
        "remaining", nargs=argparse.REMAINDER, help="Argument to pass to flent"
    )
    # --------------------------------------------------------------------------

    # ------------------------------------------------------------------ DESTROY
    parser_destroy = subparsers.add_parser("destroy", help="Destroy the deployment")
    parser_destroy.add_argument(
        "--force",
        action="store_true",
        help="Remove the remote running tansiv container. Forcing will free the g5k resources",
    )
    parser_destroy.set_defaults(func=destroy)
    # --------------------------------------------------------------------------

    # ---------------------------------------------------------------------- GEN
    parser_destroy = subparsers.add_parser(
        "gen", help="Generate the deployment file (wip)"
    )
    parser_destroy.add_argument(
        "size",
        help="Size of the deployment",
    )
    parser_destroy.add_argument(
        "out",
        help="Output file",
    )
    parser_destroy.set_defaults(func=generate_deployment)
    # --------------------------------------------------------------------------
    args = parser.parse_args()

    try:
        args.func(args)
    except Exception as e:
        parser.print_help()
        print(e)
        traceback.print_exc()
