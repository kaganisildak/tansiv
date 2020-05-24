#!/usr/bin/env bash

set -x

usage() {
    cat <<EOF
Start an UDP listener and an UDP client inside the VM.

Use $(tmux a) inside the vm to start using.

USAGE:
./boot_and_log.sh IP MAC

Positional Arguments:
  IP: the ip to use (this is likely to be correlated to the MAC but it's good enough for now)
  MAC: the mac address to use


Environment Variables:
    owned:
        QEMU: path to the qemu binary (useful to test a modified version)
    from third party (examples)
      SLIRP_DEBUG="all": activate all debug message from slirp
      G_MESSAGES_DEBUG="Slirp": glib debug filter
EOF
}

if [ -z  $QEMU ]
then
    echo "QEMU variable isn't set."
    exit 1
fi

if (( "$#" != "2" ))
then
    usage
    exit 1
fi

IP=$1
MAC=$2

VM_NAME="vm-${MAC//:/-}"

# A decent key to use
cloud_init_key=$(cat ~/.ssh/id_rsa.pub)

CLOUD_INIT_DIR=cloud-init-data-$VM_NAME
CLOUD_INIT_ISO=cloud-init-$VM_NAME.iso
mkdir -p $CLOUD_INIT_DIR

# Generate a minimal cloud-init to enable passwordless SSH access
cat <<EOF > $CLOUD_INIT_DIR/meta-data
instance-id: iid-local01
public-keys:
- $cloud_init_key"
EOF

cat <<EOF > $CLOUD_INIT_DIR/user-data
#cloud-config
hostname: $VM_NAME
local-hostname: $VM_NAME
disable_root: false
bootcmd:
- echo "-----> START of MY CLOUD INIT <---------- \n"
- echo "-----> END of MY CLOUD INIT <---------- \n"
EOF

genisoimage -output $CLOUD_INIT_ISO -volid cidata -joliet -rock $CLOUD_INIT_DIR/user-data $CLOUD_INIT_DIR/meta-data

if [ ! -f debian10-x64-min.qcow2 ]; then
    scp rennes:/grid5000/virt-images/debian10-x64-min.qcow2 .
fi

# Create the qcow2 to boot from
qemu-img create -f qcow2 -o backing_file=./debian10-x64-min.qcow2 $VM_NAME.qcow2
VM_IMAGE=$VM_NAME.qcow2
# start on the base one
# VM_IMAGE=debian10-x64-min.qcow2

TAP_NAME=tap${MAC:(-2)}
$QEMU \
  -m 1g \
  -drive file=$VM_IMAGE \
  -cdrom $CLOUD_INIT_ISO \
  -netdev tantap,src=$IP,id=mynet0,ifname=$TAP_NAME,script=no,downscript=no \
  -device e1000,netdev=mynet0,mac=$MAC