#!/usr/bin/env python3
import argparse
import ipaddress
import subprocess
import sys

parser = argparse.ArgumentParser()
parser.add_argument(
    "--create-tun",
    type=str,
    help="The name of the tun to create for the container’s packets to go through\n"
        +"There is currently no way to use a pre-existing tun in this script"
)
parser.add_argument(
    "--create-docker-network",
    help="The name of the ipvlan docker network to create from the used tun device\n"
        +"There is currently no way to use a pre-existing docker network in this script"
)
parser.add_argument(
    "--use-ip",
    type=str,
    help="The IPv4 address to use for this container and ranget to use when creating the tun interface (in CIDR notation)\n"
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

tun_name=args.create_tun
docker_network_name=args.create_docker_network

used_ip=ipaddress.ip_interface(args.use_ip)

def run_container():
    """Runs container and returns the container ID"""
    r = subprocess.run(
        ["docker", "run", "-d", # -d?
         "--network", docker_network_name, "--ip", str(used_ip.ip)]
        + sum((["--mount", margs] for margs in args.docker_mounts), start=[])
        + [args.docker_image, "/bin/sh", "-c", args.docker_program],
        stdout=subprocess.PIPE, # should still show stderr in case of error
    )
    r.check_returncode()
    return r.stdout.decode('utf-8').strip()

def create_tun():
     subprocess.run(["ip", "tuntap", "add", args.create_tun, "mode", "tun"]).check_returncode()
     try:
         subprocess.run(["ip", "link", "set", args.create_tun, "up"])
     except e:
         print(e)
         subprocess.run(["ip", "link", "delete", args.create_tun]).check_returncode()
         raise

def create_docker_network():
    """Creates the network and returns its ID"""
    r = subprocess.run([
        "docker", "network", "create", "--driver=ipvlan",
        "--subnet", str(used_ip.network),
        "--gateway", str(next(used_ip.network.hosts())),
        "-o", "parent="+tun_name,
        docker_network_name
    ], stdout=subprocess.PIPE) # should still show stderr in case of error
    r.check_returncode()
    return r.stdout.decode('utf-8').strip()

def delete_tun():
    subprocess.run(["ip", "link", "delete", tun_name]).check_returncode()

def delete_docker_network():
    subprocess.run(["docker", "network", "rm", docker_network_name]).check_returncode()

was_tun_created=False
was_docker_network_created=False
def cleanup():
    if was_docker_network_created:
        delete_docker_network()
    if was_tun_created:
        delete_tun()

try:
    create_tun()
    was_tun_created=True
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
