#!/bin/sh -ux

case "$PACKER_BUILDER_TYPE" in
  qemu) exit 0 ;;
esac

dd if=/dev/zero of=/EMPTY bs=1M
rm -f /EMPTY
sync
sync
sync

exit 0
