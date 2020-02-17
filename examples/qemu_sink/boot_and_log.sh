#!/usr/bin/env bash

# Quelques env var utiles
#
# export QEMU = <chemin vers qemu>
#
## active le debug slirp
# export SLIRP_DEBUG="all"
# export G_MESSAGES_DEBUG="Slirp"
#
## ajoute une redirection
#
# export HOSTFW=hostfwd=tcp::10022-:22
#
## active le debug VSG

set -x

if [ -z  $QEMU ]
then
    echo "QEMU variable isn't set."
    exit 1
fi

# A decent key to use
cloud_init_key=$(cat ~/.ssh/id_rsa.pub)

mkdir -p cloud-init-data

# Generate a minimal cloud-init to enable passwordless SSH access
echo "instance-id: iid-local01
public-keys:
  - $cloud_init_key" > cloud-init-data/meta-data

echo "#cloud-config
hostname: example-vm
local-hostname: example-vm
disable_root: false" > cloud-init-data/user-data

genisoimage -output cloud-init-data.iso -volid cidata -joliet -rock cloud-init-data/user-data cloud-init-data/meta-data

if [ ! -f debian10-x64-min.qcow2 ]; then
    scp rennes:/grid5000/virt-images/debian10-x64-min.qcow2 .
fi

$QEMU \
  -m 1g \
  -drive file=debian10-x64-min.qcow2 \
  -cdrom cloud-init-data.iso \
  -netdev user,id=network0,$HOSTFWD \
  -device e1000,netdev=network0
