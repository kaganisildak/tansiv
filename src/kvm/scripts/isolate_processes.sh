#! /bin/bash

set -e

# Help
usage() {
    echo "$0 <cpus_list>"
    echo "cpus_list: Comma-separated list of CPUs to use for pinning all processes"
    exit 1
}

if [ $# -ne 1 ]; then
    echo "Insufficient arguments provided"
    usage
fi

cpus_list=$(echo $1 | tr ',' ' ')

echo "Pinning all other processes to CPUs $cpus_list"

systemctl set-property --runtime -- system.slice AllowedCPUs=$1
systemctl set-property --runtime -- user.slice AllowedCPUs=$1