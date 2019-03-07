#!/bin/sh

. ./fake_vm_args.conf

exec fake_vm -a $1 -- $(eval echo \$${FAKE_VM_ARGS}_$1)
