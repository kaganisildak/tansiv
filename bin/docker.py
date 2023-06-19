#!/usr/bin/env python3
import argparse
import ipaddress
import subprocess
import sys

docker_cgroup_base="/sys/fs/cgroup/unified/docker"

parser = argparse.ArgumentParser()
#parser.add_argument(
#    "socket_name",
#    type=str,
#    help="The (unix) socket name to use for commicating with Tansiv",
#)
#parser.add_argument(
#    "--run-stopper-with-socket",
#    type=str,
#    help="The (unix) socket name to use for communication between network and stopper processes"
#)
#parser.add_argument(
#    "--offset-file",
#    type=str,
#    help="The path to use for shared-memory setting of simulation/real-time offset"
#)
parser.add_argument(
    "--create-tap",
    type=str,
    help="The name of the tap to create for the container’s packets to go through\n"
        +"There is currently no way to use a pre-existing tap in this script"
)
parser.add_argument(
    "--create-docker-network",
    help="The name of the macvlan docker network to create from the used tap device\n"
        +"There is currently no way to use a pre-existing docker network in this script"
)
parser.add_argument(
    "--use-ip",
    type=str,
    help="The IPv4 address to use for this container and ranget to use when creating the tap interface (in CIDR notation)\n"
)
parser.add_argument(
    "--docker-mounts",
    type=str,
    nargs='*',
    help="Additional mount arguments to give to docker"
)
parser.add_argument(
    "--docker-image",
    type=str,
    help="Docker image to use to run container",
)
parser.add_argument(
    "--docker-program",
    type=str,
    help="Program to run in said container"
)

args=parser.parse_args()

tap_name=args.create_tap
docker_network_name=args.create_docker_network

used_ip=ipaddress.ip_interface(args.use_ip)

def run_container():
    """Runs container and returns the container ID"""
    r = subprocess.run(
        ["docker", "run", "-d", # -d?
         "--network", docker_network_name, "--ip", str(used_ip.ip)]
        + sum((["--mount", margs] for margs in args.docker_mounts), start=[])
        + [args.docker_image, "/bin/sh", "-c", args.docker_program],
        #capture_output=True,
        stdout=subprocess.PIPE, # should still show stderr in case of error
    )
    r.check_returncode()
    return r.stdout.decode('utf-8').strip()

def create_tap():
     subprocess.run(["ip", "tuntap", "add", args.create_tap, "mode", "tap"]).check_returncode()
     try:
         #subprocess.run(["ip", "address", "add", args.use_ip,
         #    #str(used_ip.ip) + "/32",
         #    "dev", args.create_tap]).check_returncode()
         subprocess.run(["ip", "link", "set", args.create_tap, "up"])
     except e:
         print(e)
         subprocess.run(["ip", "link", "delete", args.create_tap]).check_returncode()
         raise

def create_docker_network():
    """Creates the network and returns its ID"""
    r = subprocess.run([
        "docker", "network", "create", "--driver=macvlan",
        "--subnet", str(used_ip.network),
        #str(used_ip.ip) + "/32",
        "--gateway", str(next(used_ip.network.hosts())),
        "-o", "parent="+tap_name,
        docker_network_name
    ], #capture_output=True)
       stdout=subprocess.PIPE) # should still show stderr in case of error
    r.check_returncode()
    return r.stdout.decode('utf-8').strip()

def delete_tap():
    subprocess.run(["ip", "link", "delete", tap_name]).check_returncode()

def delete_docker_network():
    subprocess.run(["docker", "network", "rm", docker_network_name]).check_returncode()

was_tap_created=False
was_docker_network_created=False
def cleanup():
    if was_docker_network_created:
        delete_docker_network()
    if was_tap_created:
        delete_tap()

try:
    create_tap()
    was_tap_created=True
    create_docker_network()
    was_docker_network_created=True
    print(run_container())
except:
    pass
#except:
#    cleanup()
#    raise
#cleanup() # Would be cleaner with `with`
# This should be replaced by a cleaner script, or a forked process calling `docker wait`

#~~TODO: run network process and stopper process (started by network process?)~~ this script will actually be started by one of them
