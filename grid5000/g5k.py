"""
Tansiv experimentation logic on G5K.

Currently different tasks are implemented

Deploy
------

- reserve a single node on G5K
- boot some VMs using :
    - tansiv
        - fork/exec some VMs and put simgrid as a bridge between them
    - notansiv
        - fork/exec some VMs and put a regular bridge in-between
- Fill some datastructure to record the current configuration

Emulate
-------

- Emulate some network latencies between the VMs
    + internally (qdisc inside the VMs are modified)
    + externally (WiP -- qdisc modification are set outside the VM / on the host bridge)

Flent
-----

- Run some flent generic benchmark and gather results

Fping
-----

- Run an fping and gather the results


Examples:

.. code-block:: python

    # deploy
    python g5k.py --env tantap_icount deploy \
        inputs/nova_cluster.xml inputs/deployment_2.xml \
        --cluster ecotype  \
        --docker_image registry.gitlab.inria.fr/msimonin/2018-vsg/tansiv:d8fa9110e5d8fcd936a0497fe6dee2b2a09337a2  \
        --walltime=05:00:00 \
        --mode=tantap

    # benchmark it
    python g5k.py flent
"""

import argparse
import json
import logging
import time
import traceback
from ipaddress import IPv4Interface
from pathlib import Path

import enoslib as en
from enoslib.api import gather_facts
from enoslib.objects import DefaultNetwork
from enoslib.service.emul.netem import (
    NetemInConstraint,
    NetemInOutSource,
    NetemOutConstraint,
)


class TantapNetwork(DefaultNetwork):
    pass


class MantapNetwork(DefaultNetwork):
    pass


# Our two networks
## one network to communicate using through a tantap or a tap
TANTAP_NETWORK = TantapNetwork("192.168.1.0/24")
# Management network (using a tap)
MANTAP_NETWORK = MantapNetwork("10.0.0.0/24")

# indexed them (EnOSlib style)
NETWORKS = dict(tantap=[TANTAP_NETWORK], mantap=[MANTAP_NETWORK])


class G5kTansivHost(en.Host):
    """
    A TansivHost is an EnOSlib Host but requires an extra ssh jump to access it

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
            G5kTansivHost(str(ip_iface.ip), ip_iface.ip.packed[-1], tansiv_node.address)
            for ip_iface in ip_ifaces
        ]
    )
    print(tansiv_roles)
    return tansiv_roles


def generate_deployment(args, env=None) -> str:
    """Generate a deployment file with size vms.

    Many things remain hardcoded (e.g physical host to map the processes).
    """
    size = int(args.size)
    out = args.out
    import xml.etree.ElementTree as ET
    from xml.etree.ElementTree import ElementTree

    platform = ET.Element("platform", dict(version="4.1"))
    for i in range(1, size + 1):
        # we start at 10.0.0.10/192.168.1.10
        descriptor = 9 + i
        vm = ET.SubElement(
            platform,
            "actor",
            dict(host=f"nova-{i}.lyon.grid5000.fr", function="vsg_vm"),
        )
        # FIXME: duplication ahead :( use EnOSlib networks here
        ET.SubElement(vm, "argument", dict(value=f"192.168.1.{descriptor}"))
        ET.SubElement(vm, "argument", dict(value="./boot.py"))
        ET.SubElement(vm, "argument", dict(value=f"--mode"))
        ET.SubElement(vm, "argument", dict(value=f"tantap"))
        ET.SubElement(vm, "argument", dict(value=f"--autoconfig_net"))
        ET.SubElement(vm, "argument", dict(value=f"--out"))
        ET.SubElement(vm, "argument", dict(value=f"tantap/vm-{descriptor}.out"))
        ET.SubElement(vm, "argument", dict(value=f"192.168.1.{descriptor}/24"))
        ET.SubElement(vm, "argument", dict(value=f"10.0.0.{descriptor}/24"))
    element_tree = ElementTree(platform)
    with open(out, "w") as f:
        f.write('<!DOCTYPE platform SYSTEM "https://simgrid.org/simgrid.dtd">')
        f.write(ET.tostring(platform, encoding="unicode"))


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
    qemu_args = args.qemu_args
    mode = args.mode

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
            src=str(image),
            dest="/tmp/tansiv/image.qcow2",
            display_name="copying base image",
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
        # we also need the notansiv wrapper (baseline tests)
        p.synchronize(
            src="notansiv.py",
            dest="/tmp/tansiv/notansiv.py",
            display_name="copying deployment file",
        )
        # we also need the constants
        p.synchronize(
            src="constants.py",
            dest="/tmp/tansiv/constants.py",
            display_name="copying deployment file",
        )
        # finally start the container
        environment = {
            "AUTOCONFIG_NET": "true",
            "IMAGE": "image.qcow2",
        }
        if qemu_args is not None:
            environment.update(QEMU_ARGS=qemu_args)

        if mode == "tantap":
            # tantap case
            # - QEMU is set in the env (Dockerfile)
            # - QEMU_ARGS is set in the env
            # - IMAGE us set in the env
            kwargs = dict(
                command="platform.xml deployment.xml --log=vm_interface.threshold:debug --log=vm_coordinator.threshold:debug"
            )
        elif mode == "tap":
            # tap case
            # - QEMU is set in the env (Dockerfile)
            # - QEMU_ARGS is set in the env
            # - IMAGE is passed using the env
            kwargs = dict(
                command="/srv/notansiv.py --boot_cmd=/srv/boot.py", entrypoint="python3"
            )
        else:
            # FIXME could be handle earlier using a enum...
            raise ValueError(f"Unknown mode of operation {mode}")

        p.docker_container(
            state="started",
            network_mode="host",
            name="tansiv",
            image=docker_image,
            volumes=["/tmp/id_rsa.pub:/root/.ssh/id_rsa.pub", "/tmp/tansiv:/srv"],
            env=environment,
            capabilities=["NET_ADMIN"],
            devices=["/dev/net/tun"],
            **kwargs,
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

    # This is specific to tantap
    tansiv_roles = build_tansiv_roles(Path(deployment), roles["tansiv"][0])

    # FIXME: handle number > 2 for tap mode

    # waiting for the tansiv vms to show up
    en.wait_for(roles=tansiv_roles)
    tansiv_networks = NETWORKS
    tansiv_roles = en.sync_info(tansiv_roles, tansiv_networks)

    # basically we end up with a tuple tansiv_roles, tansiv_networks
    # so what we get above is a EnOSlib's Tansiv Provider
    env["roles"] = roles
    env["tansiv_roles"] = tansiv_roles
    env["tansiv_networks"] = tansiv_networks
    env["args"] = args


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
        f'fping -q -C 30 -s -e {" ".join(hostnames)}',
        roles=tansiv_roles,
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

    for bench in ["tcp_download", "tcp_upload", "udp_flood"]:

        remote_dir = f"{bench}_{str(time.time_ns())}"
        start = time.time()
        with en.play_on(roles=dict(all=workers)) as p:
            p.file(state="directory", path=f"{remote_dir}")
            # recording the "real" time
            p.shell(
                " ".join(
                    [
                        f"flent {bench}",
                        "-p totals -l 30",
                        "-H {{ flent_server }}",
                        "-f csv",
                    ]
                ),
                chdir=remote_dir,
            )
        end = time.time()
        # some results
        (env.env_name / f"timing_{remote_dir}").write_text(str(end - start))
        with en.play_on(roles=dict(all=workers)) as p:
            p.shell(f"tar -czf {remote_dir}.tar.gz {remote_dir}")
            p.fetch(src=f"{remote_dir}.tar.gz", dest=f"{env.env_name}")


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


@en.enostask()
def vm_emulate(args, env=None):
    """Emulate the network condition.

    The emulation is internal to the VMs: the qdisc of the internal interfaces are modified.

    Homogeneous constraints for now.
    Note that we have options to set heterogeneous constraints in EnOSlib as well.
    """
    options = args.options
    tansiv_roles = env["tansiv_roles"]
    tansiv_networks = env["tansiv_networks"]

    netem = en.Netem().add_constraints(
        options,
        hosts=tansiv_roles["all"],
        symetric=True,
        networks=tansiv_networks["tantap"],
    )
    netem.deploy()


@en.enostask()
def host_emulate(args, env=None):
    """Emulate the network condition.

    The emulation is external to the VMs: the qdisc of the bridge
    is modified on the host machine.
    (WiP)
    """
    options = args.options
    roles = env["roles"]
    cout = NetemOutConstraint("tantap-br", options)
    cin = NetemInConstraint("tantap-br", options)
    source = NetemInOutSource(roles["tansiv"][0], constraints=set([cin, cout]))
    en.netem([source])


@en.enostask()
def dump(args, env=None):
    """Dump some environmental informations."""
    # First on the host machine
    roles = env["roles"]
    host = dict(
        docker_ps=en.run_command("docker ps", roles=roles),
        docker_inspect=en.run_command("docker inspect tansiv", roles=roles),
        ps_qemu=en.run_command("ps aux | grep qemu", roles=roles),
        qdisc=en.run_command("tc qdisc", roles=roles),
    )
    tansiv_roles = env["tansiv_roles"]
    vms = dict(qdisc=en.run_command("tc qdisc", roles=tansiv_roles))
    dump = dict(host=host, vms=vms)
    (env.env_name / "dump").write_text(json.dumps(dump))


if __name__ == "__main__":
    logging.basicConfig(level=logging.DEBUG)

    import sys

    sys.path.append(".")
    from constants import *

    parser = argparse.ArgumentParser(description="Tansiv experimentation engine")
    parser.add_argument("--env", help="env directory to use")

    # deploy --env <required> [--cluster ...] tansiv
    # deploy --env <required> [--cluster ... ] notansiv
    # ------------------------------------------------------------------- DEPLOY
    subparsers = parser.add_subparsers(help="deploy")
    parser_deploy = subparsers.add_parser(
        "deploy", help="Deploy tansiv and the associated VMs"
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
    parser_deploy.add_argument("--queue", help="Queue to use", default="default")
    parser_deploy.add_argument(
        "--docker_image",
        help="Tansiv docker image to use",
        default=DEFAULT_DOCKER_IMAGE,
    )
    parser_deploy.add_argument(
        "--image", help="Base image to use (qcow2)", default=QEMU_IMAGE
    )
    parser_deploy.add_argument("--qemu_args", help="Some qemu_args", default=QEMU_ARGS)
    parser_deploy.add_argument(
        "--mode", help="Mode of operation (tap|tantap)", default="tantap"
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

    # ------------------------------------------------------------------ VM_EMULATE
    parser_emulate = subparsers.add_parser("vm_emulate", help="emulate")
    parser_emulate.set_defaults(func=vm_emulate)
    parser_emulate.add_argument(
        "options", help="The options to pass to our (Simple)Netem (e.g 'delay 10ms')"
    )

    # ------------------------------------------------------------------ HOST_EMULATE
    parser_emulate = subparsers.add_parser("host_emulate", help="emulate")
    parser_emulate.set_defaults(func=host_emulate)
    parser_emulate.add_argument(
        "options", help="The options to pass to our (Simple)Netem (e.g 'delay 10ms')"
    )

    # ------------------------------------------------------------------ DUMP
    parser_dump = subparsers.add_parser("dump", help="Dump some infos")
    parser_dump.set_defaults(func=dump)

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
        args.func(args, env=args.env)
    except Exception as e:
        parser.print_help()
        print(e)
        traceback.print_exc()
