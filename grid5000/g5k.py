import argparse
import logging
from ipaddress import IPv4Interface
from pathlib import Path
import traceback

from enoslib import *
from enoslib.api import gather_facts
from enoslib.types import Host, Roles


def build_tansiv_roles(deployment: Path, tansiv_node: Host) -> Roles:
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

    tree = ET.parse(deployment)
    root = tree.getroot()
    ip_ifaces = sorted(
        [
            IPv4Interface(e.attrib["value"])
            for e in root.findall("./actor/argument[last()]")
        ]
    )
    tansiv_roles = dict(
        all=[
            Host(
                str(ip_iface.ip),
                alias=f"mantap{ip_iface.ip.packed[-1]}",
                user="root",
                extra=dict(
                    tansiv_alias=f"tantap{ip_iface.ip.packed[-1]}",
                    gateway=tansiv_node.address,
                    gateway_user="root",
                ),
            )
            for ip_iface in ip_ifaces
        ]
    )
    print(tansiv_roles)
    return tansiv_roles


@enostask(new=True)
def deploy(args, env=None):
    """Deploy tansiv and the associated VMs.

    idempotent.
    """
    image = args.image
    cluster = args.cluster
    platform = args.platform
    deployment = args.deployment
    queue = args.queue
    prod = G5kNetworkConf(id="id", roles=["prod"], site="nancy", type="prod")
    conf = (
        G5kConf.from_settings(
            job_name="tansiv",
            job_type="allow_classic_ssh",
            walltime="01:00:00",
            queue=queue,
        )
        .add_machine(cluster=cluster, roles=["tansiv"], nodes=1, primary_network=prod)
        .add_network_conf(prod)
    ).finalize()

    provider = G5k(conf)
    roles, _ = provider.init()

    # install docker
    docker = Docker(agent=roles["tansiv"], bind_var_docker="/tmp/docker")
    docker.deploy()
    # copy my ssh key
    pub_key = Path.home() / ".ssh" / "id_rsa.pub"
    if not pub_key.exists() or not pub_key.is_file():
        raise Exception(f"No public key found in {pub_key}")

    with play_on(roles=roles) as p:
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
        p.docker_container(
            state="started",
            network_mode="host",
            name="tansiv",
            image="registry.gitlab.inria.fr/quinson/2018-vsg/tansiv:latest",
            command="platform.xml deployment.xml",
            volumes=["/tmp/id_rsa.pub:/root/.ssh/id_rsa.pub", "/tmp/tansiv:/srv"],
            env={
                "AUTOCONFIG_NET": "true",
                "IMAGE": "image.qcow2",
            },
            capabilities=["NET_ADMIN"],
            devices=["/dev/net/tun"],
        )
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

    tansiv_roles = build_tansiv_roles(
        "../examples/qemus/deployment.xml", roles["tansiv"][0]
    )

    # waiting for the tansiv vms to show up
    wait_ssh(roles=tansiv_roles)
    env["roles"] = roles
    env["tansiv_roles"] = tansiv_roles
    env["provider"] = provider


@enostask()
def validate(args, env=None):
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
    result = run_command(
        f'fping -q -C 10 -s -e {" ".join(hostnames)}', roles=tansiv_roles
    )

    # displayng the output (the result datastructure is a bit painful to parse...ask enoslib maintainer)
    for hostname, r in result["ok"].items():
        print(f"################## <{hostname}> #################")
        # fping stats are displayed on stderr
        print(r["stderr"])
        print(f"################## </{hostname}> #################")

    for hostname, r in result["failed"].items():
        print(f"host that fails = {hostname}")


@enostask()
def destroy(args, env=None):
    provider = env["provider"]
    provider.destroy()


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
    parser_deploy.add_argument("--queue", help="Qeueue to use", default="default")

    parser_deploy.set_defaults(func=deploy)
    # --------------------------------------------------------------------------

    # ----------------------------------------------------------------- VALIDATE
    parser_validate = subparsers.add_parser("validate", help="Validate the deployment")
    parser_validate.set_defaults(func=validate)
    # --------------------------------------------------------------------------

    # ------------------------------------------------------------------ DESTROY
    parser_destroy = subparsers.add_parser("destroy", help="Destroy the deployment")
    parser_destroy.set_defaults(func=destroy)
    # --------------------------------------------------------------------------

    args = parser.parse_args()
    try:
        args.func(args)
    except Exception as e:
        parser.print_help()
        print(e)
        traceback.print_exc()