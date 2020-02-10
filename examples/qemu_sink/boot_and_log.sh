set -x
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

# boot it with filter-dump object
# export SLIRP_DEBUG="all"
# export G_MESSAGES_DEBUG="Slirp"
export QEMU=/home/msimonin/workspace/repos/qemu/build/x86_64-softmmu/qemu-system-x86_64
$QEMU \
  -m 1g \
  -drive file=debian10-x64-min.qcow2 \
  -cdrom cloud-init-data.iso \
  -netdev user,id=network0,hostfwd=tcp::10022-:22 \
  -device e1000,netdev=network0
