#!/usr/bin/env bash

set -x

usage() {
    cat <<EOF
Start an UDP listener and an UDP client inside the VM.

Use $(tmux a) inside the vm to start using.

USAGE:
./boot_and_log.sh LISTEN_PORT CONNECT_HOST CONNECT_PORT

Positional Arguments:

    LISTEN_HOST: start an UDP server that will listen on this address in the VM
    LISTEN_PORT: start an UDP server that will listen on this port in the VM
    CONNECT_HOST: The UDP client will use this a foreign address
    CONNECT_PORT: The UDP client will use this as foreign port

Environment Variables:
    owned:
        QEMU: path to the qemu binary (useful to test a modified version)
    from third party (examples)
      SLIRP_DEBUG="all": activate all debug message from slirp
      G_MESSAGES_DEBUG="Slirp": glib debug filter
      HOSTFWD=hostfwd=tcp::10022-:22: add a redirection
EOF
}

if [ -z  $QEMU ]
then
    echo "QEMU variable isn't set."
    exit 1
fi

if (( "$#" != "4" ))
then
    usage
    exit 1
fi
LISTEN_HOST=$1
shift

LISTEN_PORT=$1
shift

CONNECT_HOST=$1
shift

CONNECT_PORT=$1
shift

VM_NAME="vm-$LISTEN_PORT"

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
- apt update && apt install -y tmux netcat-openbsd
- tmux new-session -s tansiv -d "echo -e 'listener $LISTEN_HOST->localhost:$LISTEN_PORT\n'; nc -u -l 0.0.0.0 $LISTEN_PORT"
- tmux new-session -s tansiv2 -d "echo -e 'client $CONNECT_HOST:$CONNECT_PORT\n'; nc -u $CONNECT_HOST $CONNECT_PORT"
- tmux move-pane -s tansiv2 -t tansiv
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

# redir one udp port passed in parameter:;
UDP_REDIR="hostfwd=udp:$LISTEN_HOST:$LISTEN_PORT-:$LISTEN_PORT"
TCP_REDIR="hostfwd=tcp::1$LISTEN_PORT-:22"
if [ -z "$HOSTFWD"]
then
    HOSTFWD=$TCP_REDIR,$UDP_REDIR
else
    HOSTFWD="$HOSTFWD,$TCP_REDIR,$UDP_REDIR"
fi

$QEMU \
  -m 1g \
  -drive file=$VM_IMAGE \
  -cdrom $CLOUD_INIT_ISO \
  -netdev user,id=network0,$HOSTFWD \
  -device e1000,netdev=network0
