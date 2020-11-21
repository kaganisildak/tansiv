#!/usr/bin/env bash

set -x

usage() {
    cat <<EOF
USAGE:
./boot_and_log.sh IP MAC MAC2

Boots a VM. Use two nics: one for tantap the other for a regular tap (management interface).
The mapping mac <-> ip must be set in your dhcp.
You can use libvirt network to get the bridge and dhcp up and running.

Positional Arguments:
  IP   : The IP to use for the tantap interface. This will the source of the vsg packet.
  MAC  : The mac address to use for the tantap interface.
         The tap to use is named based on the last byte of the mac.
  MAC2 : The mac address to use for the management interface (regular tantap).
         The tap to use is named based on the last byte of the mac.

Environment Variables:
    owned:

        QEMU: path to the qemu binary (useful to test a modified version)
        IMAGE: path to a qcow2 or raw image disk (to serve as backing file for the disk images)

    from third party (examples)
      SLIRP_DEBUG="all": activate all debug message from slirp
      G_MESSAGES_DEBUG="Slirp": glib debug filter

NOTE:
  - create tuntap before:
  - for tap in {tap10,tap11}
    do
        sudo ip tuntap add $tap mode tap user msimonin && sudo ip link set $tap master tantap0 && sudo ip link set $tap up
    done
  - for tap in {tap20,tap21}
    do
        sudo ip tuntap add $tap mode tap user msimonin && sudo ip link set $tap master tantap0-mgmt && sudo ip link set $tap up
    done
EOF
}

if [ -z  $QEMU ]
then
    echo "QEMU variable isn't set."
    exit 1
fi

if [ -z $IMAGE ]
then
    echo "IMAGE disk isn't set"
    exit 1
fi

if (( "$#" != "3" ))
then
    usage
    exit 1
fi

IP=$1
MAC=$2
MAC2=$3

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
- echo "192.168.120.10    t10" >> /etc/hosts
- echo "192.168.120.11    t11" >> /etc/hosts
- echo "192.168.121.20    m10" >> /etc/hosts
- echo "192.168.121.21    m11" >> /etc/hosts
- echo 127.0.0.1 $VM_NAME >> /etc/hosts
EOF

cat <<EOF > $CLOUD_INIT_DIR/network-config
version: 2
ethernets:
  ens3:
    dhcp4: true
  ens4:
    dhcp4: true
EOF


genisoimage -output $CLOUD_INIT_ISO -volid cidata -joliet -rock $CLOUD_INIT_DIR/user-data $CLOUD_INIT_DIR/meta-data $CLOUD_INIT_DIR/network-config

# Create the qcow2 to boot from
qemu-img create -f qcow2 -o backing_file=$IMAGE $VM_NAME.qcow2
VM_IMAGE=$VM_NAME.qcow2
# start on the base one
# VM_IMAGE=debian10-x64-min.qcow2

TAP_NAME=tap${MAC:(-2)}
TAP2_NAME=tap${MAC2:(-2)}
$QEMU \
  --icount shift=1,sleep=on \
  -rtc clock=vm \
  --vsg mynet0,src=$IP \
  -m 1g \
  -drive file=$VM_IMAGE \
  -cdrom $CLOUD_INIT_ISO \
  -netdev tantap,src=$IP,id=mynet0,ifname=$TAP_NAME,script=no,downscript=no \
  -device e1000,netdev=mynet0,mac=$MAC \
  -netdev tap,id=mynet1,ifname=$TAP2_NAME,script=no,downscript=no \
  -device e1000,netdev=mynet1,mac=$MAC2