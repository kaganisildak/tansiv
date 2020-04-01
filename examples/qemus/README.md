# Compilation

1. compiler `tansiv`
2. compiler `qemu` modifié avec slirp modifié
Pour l'instant mon workflow est le suivant:
- Installation de libvsg dans /opt: `make DESTDIR=/opt/tansiv install`
- Compilation de qemu avec un libslirp modifié:
  - un truc comme ça: `rsync -avz ../../2018-vsg/src/slirp/* ../slirp/. && make clean && make -j8`
3. lancer tansiv en positionant le chemin vers le nouveau binaire qemu:
```
export QEMU=/home/msimonin/workspace/repos/qemu/build/x86_64-softmmu/qemu-system-x86_64
./tansiv ../platform/nova_cluster.xml ../examples/qemu_sink/deployment.xml --log=vm_interface.threshold:debug --log=vm_coordinator.threshold:debug
```

Exemple:
- `nc -u 10.0.2.2 12345` dans la vm
- sortie
```
VSG_LOG=0 ./tansiv ../platform/nova_cluster.xml ../examples/qemu_sink/deployment.xml --log=vm_interface.threshold:debug --log=vm_coordinator.threshold:debug
13:42:34 INFO  /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:46: Welcome to VSG
[0.000000] [vm_interface/INFO] socket created
[0.000000] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:62: [vm_interface/VERBOSE] socket binded
[0.000000] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:68: [vm_interface/VERBOSE] listen on socket
[nova-1.lyon.grid5000.fr:vsg_vm:(2) 0.000000] [vm_coordinator/INFO] running receiver
[nova-1.lyon.grid5000.fr:vsg_vm:(2) 0.000000] [vm_interface/INFO] fork and exec of [./examples/qemu_sink/boot_and_log.sh ./examples/qemu_sink/boot_and_log.sh]
[nova-1.lyon.grid5000.fr:vsg_vm:(2) 0.000000] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:110: [vm_interface/VERBOSE] fork done for VM 127.0.0.2
[nova-1.lyon.grid5000.fr:vsg_vm:(2) 0.000000] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:128: [vm_interface/VERBOSE] vm sockets are down
+ [ -z /home/msimonin/workspace/repos/qemu/build/x86_64-softmmu/qemu-system-x86_64 ]
+ cat /home/msimonin/.ssh/id_rsa.pub
+ cloud_init_key=ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAAEAADfNo+lX5Wb8inFG8/N/OiwaDz29Lwc7N/nZ2DpZOJsP+gCUXtVI1dEt6RMMWn0NzD92Lg23eYd0E8cIbVyEeuODj94ClwPFxtca2nzrYd9holkhT0kS6vl6DRxhRd7jvKVYvc56GrvCGtupNi1lwboiL4yxKDs1rBxMDtbY9tvwGdBfcCWxLVxwMhU9EwyhbVTIY2H5bA6mcIQ8kmfXUnOw7Isq3tcg+fzGxPpo5M53SLVNz1x52uBmbgdMmYHpnlkuiv18ySYnZPYrU22MZjhD7AAMqtdgsNdBAOudRbMr63iDZ+9ApQkeIFsQLQXn/3p9phtBlKqkQR2uJ5NkuvzjxA27iIt7NPjjLBEPOknOSwXYcBTWBqATRJRecwot2+FicgeqY88ngQiXk5r+k2hZ/PSiieWAyWtgRgFBjP1uEO4huROt12BE6B4QltJJhxKj6tq6II4ZXOSUqIQU0oRoNSWvvdU4llgB0UcOZyEryITGjxeWs7d9kpP+dbPF+QKTAWo414mHO6oo9CtyjGDbChf1xTDt0Poy34+5/WW3TeRft7PRRVQu7nyu3v2paf0bp+1cdDUVhz3DO4385MYSUInHhkeNzkujjd6Inmb/TSbGD+m6WXYHTU04XNmN1KYK17t/VRauMF7dPFLtjTd/r2LWW0M+jz45TQoD+2TOMyOWYX33GnZwGdHOHcklBWcK8hntqg4wz5+f/+cA9Cue1Ny993XzpNd2jhcxODDNqOmUuaNJ1bS4EOP8HrNPRB8G7+PaPgAaIgzaBFGPmjhFs3uGCItaYircr0n8KK6hiqte0eAQvWMpd327QH28EsBGeQ+c9bdoAsxJyg2deXgM1ZPPrmgikQQFden/RCT40uba8UDPvxeHpoFwGIBk+9uUv820vAWaWRx1lgw1n2l7Dlwf/hU2JVnD43fMOUhwV2g5+FTs7f3dGsdB35GHbIoNhd8HQHcTAqFdoJl2LaS5Clzcni9kEKb0TGqYsZjnEmiGeWUH/jiDpjkxEJYXyQIfVv1AysanwYls5oIEUFx+fAJQkjVu8yVccAnbzFSbY5gsMcNqYZf4Wb+sHsmjgFU4rm9s6t05LGi1XuQs8vGX3CtTY5v/4s4ITLGFsyx8iWB14K3Q+9P78GmruIJiTCJr/yW3au7ye01fMdIxoN4ezRFK7yUqO2KBIFo8IlLFem3i8bfkLeEjcytSL6bhGd5651bkEStUPATRD3DzFtuJnns1WN/dgAurNaktHtJPPyqudhRcGOO8gY9bo212cNDGnkW5PZHi7E3l+CyAlLG6XkE3zWXku+ZMksbj2YJ2DX2e8DBlj3zwIcB9fAIxpCtXBOBSpZxrKkoUabtBfU= msimonin@talouette
+ mkdir -p cloud-init-data
+ echo instance-id: iid-local01
public-keys:
  - ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAAEAADfNo+lX5Wb8inFG8/N/OiwaDz29Lwc7N/nZ2DpZOJsP+gCUXtVI1dEt6RMMWn0NzD92Lg23eYd0E8cIbVyEeuODj94ClwPFxtca2nzrYd9holkhT0kS6vl6DRxhRd7jvKVYvc56GrvCGtupNi1lwboiL4yxKDs1rBxMDtbY9tvwGdBfcCWxLVxwMhU9EwyhbVTIY2H5bA6mcIQ8kmfXUnOw7Isq3tcg+fzGxPpo5M53SLVNz1x52uBmbgdMmYHpnlkuiv18ySYnZPYrU22MZjhD7AAMqtdgsNdBAOudRbMr63iDZ+9ApQkeIFsQLQXn/3p9phtBlKqkQR2uJ5NkuvzjxA27iIt7NPjjLBEPOknOSwXYcBTWBqATRJRecwot2+FicgeqY88ngQiXk5r+k2hZ/PSiieWAyWtgRgFBjP1uEO4huROt12BE6B4QltJJhxKj6tq6II4ZXOSUqIQU0oRoNSWvvdU4llgB0UcOZyEryITGjxeWs7d9kpP+dbPF+QKTAWo414mHO6oo9CtyjGDbChf1xTDt0Poy34+5/WW3TeRft7PRRVQu7nyu3v2paf0bp+1cdDUVhz3DO4385MYSUInHhkeNzkujjd6Inmb/TSbGD+m6WXYHTU04XNmN1KYK17t/VRauMF7dPFLtjTd/r2LWW0M+jz45TQoD+2TOMyOWYX33GnZwGdHOHcklBWcK8hntqg4wz5+f/+cA9Cue1Ny993XzpNd2jhcxODDNqOmUuaNJ1bS4EOP8HrNPRB8G7+PaPgAaIgzaBFGPmjhFs3uGCItaYircr0n8KK6hiqte0eAQvWMpd327QH28EsBGeQ+c9bdoAsxJyg2deXgM1ZPPrmgikQQFden/RCT40uba8UDPvxeHpoFwGIBk+9uUv820vAWaWRx1lgw1n2l7Dlwf/hU2JVnD43fMOUhwV2g5+FTs7f3dGsdB35GHbIoNhd8HQHcTAqFdoJl2LaS5Clzcni9kEKb0TGqYsZjnEmiGeWUH/jiDpjkxEJYXyQIfVv1AysanwYls5oIEUFx+fAJQkjVu8yVccAnbzFSbY5gsMcNqYZf4Wb+sHsmjgFU4rm9s6t05LGi1XuQs8vGX3CtTY5v/4s4ITLGFsyx8iWB14K3Q+9P78GmruIJiTCJr/yW3au7ye01fMdIxoN4ezRFK7yUqO2KBIFo8IlLFem3i8bfkLeEjcytSL6bhGd5651bkEStUPATRD3DzFtuJnns1WN/dgAurNaktHtJPPyqudhRcGOO8gY9bo212cNDGnkW5PZHi7E3l+CyAlLG6XkE3zWXku+ZMksbj2YJ2DX2e8DBlj3zwIcB9fAIxpCtXBOBSpZxrKkoUabtBfU= msimonin@talouette
+ echo #cloud-config
hostname: example-vm
local-hostname: example-vm
disable_root: false
+ genisoimage -output cloud-init-data.iso -volid cidata -joliet -rock cloud-init-data/user-data cloud-init-data/meta-data
I: -input-charset not specified, using utf-8 (detected in locale settings)
Total translation table size: 0
Total rockridge attributes bytes: 331
Total directory bytes: 0
Path table size(bytes): 10
Max brk space used 0
183 extents written (0 MB)
+ [ ! -f debian10-x64-min.qcow2 ]
+ /home/msimonin/workspace/repos/qemu/build/x86_64-softmmu/qemu-system-x86_64 -m 1g -drive file=debian10-x64-min.qcow2 -cdrom cloud-init-data.iso -netdev user,id=network0,hostfwd=tcp::10022-:22 -device e1000,netdev=network0
13:42:34 INFO  /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:46: Welcome to VSG
13:42:34 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:52: Create an UNIX socket to simgrid_connection_socket
13:42:34 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:63: vsg connection established [fd=21]
[nova-1.lyon.grid5000.fr:vsg_vm:(2) 0.000000] [vm_interface/INFO] connection for VM 127.0.0.2 established
[nova-2.lyon.grid5000.fr:vsg_vm:(3) 0.000000] [vm_coordinator/INFO] running receiver
[nova-2.lyon.grid5000.fr:vsg_vm:(3) 0.000000] [vm_interface/INFO] fork and exec of [./examples/qemu_sink/sink ./examples/qemu_sink/sink]
[nova-2.lyon.grid5000.fr:vsg_vm:(3) 0.000000] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:110: [vm_interface/VERBOSE] fork done for VM 127.0.0.1
[nova-2.lyon.grid5000.fr:vsg_vm:(3) 0.000000] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:128: [vm_interface/VERBOSE] vm sockets are down
13:42:34 INFO  /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:46: Welcome to VSG
13:42:34 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:52: Create an UNIX socket to simgrid_connection_socket
13:42:34 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:63: vsg connection established [fd=3]
[nova-2.lyon.grid5000.fr:vsg_vm:(3) 0.000000] [vm_interface/INFO] connection for VM 127.0.0.1 established
13:42:34 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:82: VSG waiting order
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.000000] [vm_coordinator/INFO] the minimum latency on the network is 0.000200 sec
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.000000] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:136: [vm_coordinator/DEBUG] simulating to time 0.000200
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.000000] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:150: [vm_interface/DEBUG] asking all the VMs to go to time 0.000200 (0.000200)
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.000000] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:162: [vm_interface/DEBUG] getting the message send by the VMs
13:42:34 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:100: VSG_GOTO_DEADLINE recv
13:42:34 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
13:42:34 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:82: VSG waiting order
13:43:37 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:117: VSG_SEND_PACKET send time[s=0, us=1] dest[127.0.0.53] message_length[28]
13:43:37 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.000000] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:183: [vm_interface/VERBOSE] getting a message from VM 127.0.0.2
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.000000] [vm_interface/INFO] got the message [�] (size 28) from VM [127.0.0.2] to VM [127.0.0.53]
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.000000] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:227: [vm_interface/DEBUG] forwarding all the messages to SimGrid
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.000000] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:148: [vm_coordinator/DEBUG] going to time 0.000001
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.000010] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:163: [vm_coordinator/WARNING] the VM 127.0.0.2 tries to send a message to the unknown VM 127.0.0.53
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.000200] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:136: [vm_coordinator/DEBUG] simulating to time 0.000400
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.000200] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:150: [vm_interface/DEBUG] asking all the VMs to go to time 0.000400 (0.000400)
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.000200] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:162: [vm_interface/DEBUG] getting the message send by the VMs
13:43:37 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:100: VSG_GOTO_DEADLINE recv
13:43:37 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
13:43:37 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:82: VSG waiting order
13:43:37 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:117: VSG_SEND_PACKET send time[s=0, us=201] dest[127.0.0.53] message_length[28]
13:43:37 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.000200] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:183: [vm_interface/VERBOSE] getting a message from VM 127.0.0.2
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.000200] [vm_interface/INFO] got the message [(�] (size 28) from VM [127.0.0.2] to VM [127.0.0.53]
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.000200] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:227: [vm_interface/DEBUG] forwarding all the messages to SimGrid
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.000200] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:148: [vm_coordinator/DEBUG] going to time 0.000201
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.000210] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:163: [vm_coordinator/WARNING] the VM 127.0.0.2 tries to send a message to the unknown VM 127.0.0.53
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.000400] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:136: [vm_coordinator/DEBUG] simulating to time 0.000600
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.000400] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:150: [vm_interface/DEBUG] asking all the VMs to go to time 0.000600 (0.000600)
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.000400] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:162: [vm_interface/DEBUG] getting the message send by the VMs
13:43:37 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:100: VSG_GOTO_DEADLINE recv
13:43:37 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
13:43:37 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:82: VSG waiting order
13:43:37 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:117: VSG_SEND_PACKET send time[s=0, us=401] dest[127.0.0.53] message_length[28]
13:43:37 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.000400] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:183: [vm_interface/VERBOSE] getting a message from VM 127.0.0.2
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.000400] [vm_interface/INFO] got the message [�] (size 28) from VM [127.0.0.2] to VM [127.0.0.53]
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.000400] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:227: [vm_interface/DEBUG] forwarding all the messages to SimGrid
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.000400] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:148: [vm_coordinator/DEBUG] going to time 0.000401
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.000410] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:163: [vm_coordinator/WARNING] the VM 127.0.0.2 tries to send a message to the unknown VM 127.0.0.53
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.000600] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:136: [vm_coordinator/DEBUG] simulating to time 0.000800
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.000600] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:150: [vm_interface/DEBUG] asking all the VMs to go to time 0.000800 (0.000800)
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.000600] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:162: [vm_interface/DEBUG] getting the message send by the VMs
13:43:37 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:100: VSG_GOTO_DEADLINE recv
13:43:37 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
13:43:37 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:82: VSG waiting order
13:43:37 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:117: VSG_SEND_PACKET send time[s=0, us=601] dest[127.0.0.53] message_length[28]
13:43:37 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.000600] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:183: [vm_interface/VERBOSE] getting a message from VM 127.0.0.2
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.000600] [vm_interface/INFO] got the message [(�] (size 28) from VM [127.0.0.2] to VM [127.0.0.53]
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.000600] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:227: [vm_interface/DEBUG] forwarding all the messages to SimGrid
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.000600] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:148: [vm_coordinator/DEBUG] going to time 0.000601
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.000610] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:163: [vm_coordinator/WARNING] the VM 127.0.0.2 tries to send a message to the unknown VM 127.0.0.53
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.000800] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:136: [vm_coordinator/DEBUG] simulating to time 0.001000
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.000800] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:150: [vm_interface/DEBUG] asking all the VMs to go to time 0.001000 (0.001000)
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.000800] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:162: [vm_interface/DEBUG] getting the message send by the VMs
13:43:37 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:100: VSG_GOTO_DEADLINE recv
13:43:37 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
13:43:37 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:82: VSG waiting order
13:43:49 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:117: VSG_SEND_PACKET send time[s=0, us=801] dest[127.0.0.53] message_length[39]
13:43:49 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.000800] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:183: [vm_interface/VERBOSE] getting a message from VM 127.0.0.2
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.000800] [vm_interface/INFO] got the message [HU] (size 39) from VM [127.0.0.2] to VM [127.0.0.53]
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.000800] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:227: [vm_interface/DEBUG] forwarding all the messages to SimGrid
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.000800] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:148: [vm_coordinator/DEBUG] going to time 0.000801
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.000810] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:163: [vm_coordinator/WARNING] the VM 127.0.0.2 tries to send a message to the unknown VM 127.0.0.53
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.001000] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:136: [vm_coordinator/DEBUG] simulating to time 0.001200
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.001000] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:150: [vm_interface/DEBUG] asking all the VMs to go to time 0.001200 (0.001200)
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.001000] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:162: [vm_interface/DEBUG] getting the message send by the VMs
13:43:49 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:100: VSG_GOTO_DEADLINE recv
13:43:49 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
13:43:49 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:82: VSG waiting order
13:43:49 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:117: VSG_SEND_PACKET send time[s=0, us=1001] dest[127.0.0.53] message_length[39]
13:43:49 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.001000] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:183: [vm_interface/VERBOSE] getting a message from VM 127.0.0.2
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.001000] [vm_interface/INFO] got the message [�p] (size 39) from VM [127.0.0.2] to VM [127.0.0.53]
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.001000] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:227: [vm_interface/DEBUG] forwarding all the messages to SimGrid
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.001000] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:148: [vm_coordinator/DEBUG] going to time 0.001001
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.001010] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:163: [vm_coordinator/WARNING] the VM 127.0.0.2 tries to send a message to the unknown VM 127.0.0.53
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.001200] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:136: [vm_coordinator/DEBUG] simulating to time 0.001400
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.001200] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:150: [vm_interface/DEBUG] asking all the VMs to go to time 0.001400 (0.001400)
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.001200] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:162: [vm_interface/DEBUG] getting the message send by the VMs
13:43:49 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:100: VSG_GOTO_DEADLINE recv
13:43:49 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
13:43:49 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:82: VSG waiting order
13:43:49 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:117: VSG_SEND_PACKET send time[s=0, us=1201] dest[91.121.7.182] message_length[48]
13:43:49 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.001200] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:183: [vm_interface/VERBOSE] getting a message from VM 127.0.0.2
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.001200] [vm_interface/INFO] got the message [#] (size 48) from VM [127.0.0.2] to VM [91.121.7.182]
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.001200] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:227: [vm_interface/DEBUG] forwarding all the messages to SimGrid
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.001200] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:148: [vm_coordinator/DEBUG] going to time 0.001201
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.001210] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:163: [vm_coordinator/WARNING] the VM 127.0.0.2 tries to send a message to the unknown VM 91.121.7.182
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.001400] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:136: [vm_coordinator/DEBUG] simulating to time 0.001600
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.001400] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:150: [vm_interface/DEBUG] asking all the VMs to go to time 0.001600 (0.001600)
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.001400] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:162: [vm_interface/DEBUG] getting the message send by the VMs
13:43:49 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:100: VSG_GOTO_DEADLINE recv
13:43:49 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
13:43:49 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:82: VSG waiting order
13:44:04 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:117: VSG_SEND_PACKET send time[s=0, us=1401] dest[127.0.0.1] message_length[5]
13:44:04 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.001400] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:183: [vm_interface/VERBOSE] getting a message from VM 127.0.0.2
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.001400] [vm_interface/INFO] got the message [plop
] (size 5) from VM [127.0.0.2] to VM [127.0.0.1]
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.001400] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:227: [vm_interface/DEBUG] forwarding all the messages to SimGrid
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.001400] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:148: [vm_coordinator/DEBUG] going to time 0.001401
[nova-1.lyon.grid5000.fr:sender:(4) 0.001410] [vm_coordinator/INFO] sending [plop
] (size 5) from vm [127.0.0.2], to vm [127.0.0.1] (on pm [nova-2.lyon.grid5000.fr])
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.001600] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:136: [vm_coordinator/DEBUG] simulating to time 0.001800
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.001600] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:150: [vm_interface/DEBUG] asking all the VMs to go to time 0.001800 (0.001800)
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.001600] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:162: [vm_interface/DEBUG] getting the message send by the VMs
13:44:04 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:100: VSG_GOTO_DEADLINE recv
13:44:04 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
13:44:04 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:82: VSG waiting order
13:44:11 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:117: VSG_SEND_PACKET send time[s=0, us=1601] dest[127.0.0.1] message_length[2]
13:44:11 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.001600] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:183: [vm_interface/VERBOSE] getting a message from VM 127.0.0.2
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.001600] [vm_interface/INFO] got the message [a
] (size 2) from VM [127.0.0.2] to VM [127.0.0.1]
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.001600] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:227: [vm_interface/DEBUG] forwarding all the messages to SimGrid
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.001600] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:148: [vm_coordinator/DEBUG] going to time 0.001601
[nova-1.lyon.grid5000.fr:sender:(5) 0.001610] [vm_coordinator/INFO] sending [a
] (size 2) from vm [127.0.0.2], to vm [127.0.0.1] (on pm [nova-2.lyon.grid5000.fr])
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.001800] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:136: [vm_coordinator/DEBUG] simulating to time 0.002000
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.001800] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:150: [vm_interface/DEBUG] asking all the VMs to go to time 0.002000 (0.002000)
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.001800] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:162: [vm_interface/DEBUG] getting the message send by the VMs
13:44:11 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:100: VSG_GOTO_DEADLINE recv
13:44:11 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
13:44:11 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:82: VSG waiting order
13:44:13 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:117: VSG_SEND_PACKET send time[s=0, us=1801] dest[127.0.0.1] message_length[6]
13:44:13 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.001800] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:183: [vm_interface/VERBOSE] getting a message from VM 127.0.0.2
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.001800] [vm_interface/INFO] got the message [hello
] (size 6) from VM [127.0.0.2] to VM [127.0.0.1]
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.001800] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:227: [vm_interface/DEBUG] forwarding all the messages to SimGrid
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.001800] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:148: [vm_coordinator/DEBUG] going to time 0.001801
[nova-1.lyon.grid5000.fr:sender:(6) 0.001810] [vm_coordinator/INFO] sending [hello
] (size 6) from vm [127.0.0.2], to vm [127.0.0.1] (on pm [nova-2.lyon.grid5000.fr])
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.002000] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:136: [vm_coordinator/DEBUG] simulating to time 0.002200
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.002000] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:150: [vm_interface/DEBUG] asking all the VMs to go to time 0.002200 (0.002200)
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.002000] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:162: [vm_interface/DEBUG] getting the message send by the VMs
13:44:13 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:100: VSG_GOTO_DEADLINE recv
13:44:13 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
13:44:13 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:82: VSG waiting order
13:44:15 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:117: VSG_SEND_PACKET send time[s=0, us=2001] dest[127.0.0.1] message_length[4]
13:44:15 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.002000] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:183: [vm_interface/VERBOSE] getting a message from VM 127.0.0.2
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.002000] [vm_interface/INFO] got the message [foo
] (size 4) from VM [127.0.0.2] to VM [127.0.0.1]
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.002000] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:227: [vm_interface/DEBUG] forwarding all the messages to SimGrid
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.002000] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:148: [vm_coordinator/DEBUG] going to time 0.002001
[nova-1.lyon.grid5000.fr:sender:(7) 0.002010] [vm_coordinator/INFO] sending [foo
] (size 4) from vm [127.0.0.2], to vm [127.0.0.1] (on pm [nova-2.lyon.grid5000.fr])
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.002200] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:136: [vm_coordinator/DEBUG] simulating to time 0.002400
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.002200] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:150: [vm_interface/DEBUG] asking all the VMs to go to time 0.002400 (0.002400)
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.002200] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:162: [vm_interface/DEBUG] getting the message send by the VMs
13:44:15 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:100: VSG_GOTO_DEADLINE recv
13:44:15 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
13:44:15 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:82: VSG waiting order
13:44:15 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:117: VSG_SEND_PACKET send time[s=0, us=2201] dest[127.0.0.1] message_length[1]
13:44:15 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.002200] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:183: [vm_interface/VERBOSE] getting a message from VM 127.0.0.2
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.002200] [vm_interface/INFO] got the message [
] (size 1) from VM [127.0.0.2] to VM [127.0.0.1]
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.002200] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:227: [vm_interface/DEBUG] forwarding all the messages to SimGrid
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.002200] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:148: [vm_coordinator/DEBUG] going to time 0.002201
[nova-1.lyon.grid5000.fr:sender:(8) 0.002210] [vm_coordinator/INFO] sending [
] (size 1) from vm [127.0.0.2], to vm [127.0.0.1] (on pm [nova-2.lyon.grid5000.fr])
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.002400] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:136: [vm_coordinator/DEBUG] simulating to time 0.002600
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.002400] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:150: [vm_interface/DEBUG] asking all the VMs to go to time 0.002600 (0.002600)
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.002400] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:162: [vm_interface/DEBUG] getting the message send by the VMs
13:44:15 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:100: VSG_GOTO_DEADLINE recv
13:44:15 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
13:44:15 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:82: VSG waiting order
13:44:16 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:117: VSG_SEND_PACKET send time[s=0, us=2401] dest[127.0.0.1] message_length[1]
13:44:16 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.002400] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:183: [vm_interface/VERBOSE] getting a message from VM 127.0.0.2
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.002400] [vm_interface/INFO] got the message [
] (size 1) from VM [127.0.0.2] to VM [127.0.0.1]
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.002400] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:227: [vm_interface/DEBUG] forwarding all the messages to SimGrid
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.002400] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:148: [vm_coordinator/DEBUG] going to time 0.002401
[nova-1.lyon.grid5000.fr:sender:(9) 0.002410] [vm_coordinator/INFO] sending [
] (size 1) from vm [127.0.0.2], to vm [127.0.0.1] (on pm [nova-2.lyon.grid5000.fr])
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.002600] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:136: [vm_coordinator/DEBUG] simulating to time 0.002800
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.002600] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:150: [vm_interface/DEBUG] asking all the VMs to go to time 0.002800 (0.002800)
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.002600] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:162: [vm_interface/DEBUG] getting the message send by the VMs
13:44:16 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:100: VSG_GOTO_DEADLINE recv
13:44:16 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
13:44:16 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:82: VSG waiting order
13:44:16 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:117: VSG_SEND_PACKET send time[s=0, us=2601] dest[127.0.0.1] message_length[1]
13:44:16 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.002600] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:183: [vm_interface/VERBOSE] getting a message from VM 127.0.0.2
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.002600] [vm_interface/INFO] got the message [
] (size 1) from VM [127.0.0.2] to VM [127.0.0.1]
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.002600] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:227: [vm_interface/DEBUG] forwarding all the messages to SimGrid
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.002600] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:148: [vm_coordinator/DEBUG] going to time 0.002601
[nova-1.lyon.grid5000.fr:sender:(10) 0.002610] [vm_coordinator/INFO] sending [
] (size 1) from vm [127.0.0.2], to vm [127.0.0.1] (on pm [nova-2.lyon.grid5000.fr])
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.002800] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:136: [vm_coordinator/DEBUG] simulating to time 0.003000
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.002800] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:150: [vm_interface/DEBUG] asking all the VMs to go to time 0.003000 (0.003000)
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.002800] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:162: [vm_interface/DEBUG] getting the message send by the VMs
13:44:16 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:100: VSG_GOTO_DEADLINE recv
13:44:16 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
13:44:16 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:82: VSG waiting order
13:44:16 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:117: VSG_SEND_PACKET send time[s=0, us=2801] dest[127.0.0.1] message_length[1]
13:44:16 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.002800] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:183: [vm_interface/VERBOSE] getting a message from VM 127.0.0.2
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.002800] [vm_interface/INFO] got the message [
] (size 1) from VM [127.0.0.2] to VM [127.0.0.1]
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.002800] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:227: [vm_interface/DEBUG] forwarding all the messages to SimGrid
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.002800] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:148: [vm_coordinator/DEBUG] going to time 0.002801
[nova-1.lyon.grid5000.fr:sender:(11) 0.002810] [vm_coordinator/INFO] sending [
] (size 1) from vm [127.0.0.2], to vm [127.0.0.1] (on pm [nova-2.lyon.grid5000.fr])
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.003000] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:136: [vm_coordinator/DEBUG] simulating to time 0.003200
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.003000] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:150: [vm_interface/DEBUG] asking all the VMs to go to time 0.003200 (0.003200)
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.003000] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:162: [vm_interface/DEBUG] getting the message send by the VMs
13:44:16 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:100: VSG_GOTO_DEADLINE recv
13:44:16 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
13:44:16 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:82: VSG waiting order
13:44:17 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:117: VSG_SEND_PACKET send time[s=0, us=3001] dest[127.0.0.1] message_length[1]
13:44:17 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.003000] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:183: [vm_interface/VERBOSE] getting a message from VM 127.0.0.2
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.003000] [vm_interface/INFO] got the message [
] (size 1) from VM [127.0.0.2] to VM [127.0.0.1]
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.003000] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:227: [vm_interface/DEBUG] forwarding all the messages to SimGrid
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.003000] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:148: [vm_coordinator/DEBUG] going to time 0.003001
[nova-1.lyon.grid5000.fr:sender:(12) 0.003010] [vm_coordinator/INFO] sending [
] (size 1) from vm [127.0.0.2], to vm [127.0.0.1] (on pm [nova-2.lyon.grid5000.fr])
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.003200] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:136: [vm_coordinator/DEBUG] simulating to time 0.003400
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.003200] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:150: [vm_interface/DEBUG] asking all the VMs to go to time 0.003400 (0.003400)
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.003200] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:162: [vm_interface/DEBUG] getting the message send by the VMs
13:44:17 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:100: VSG_GOTO_DEADLINE recv
13:44:17 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
13:44:17 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:82: VSG waiting order
13:44:17 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:117: VSG_SEND_PACKET send time[s=0, us=3201] dest[127.0.0.1] message_length[1]
13:44:17 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.003200] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:183: [vm_interface/VERBOSE] getting a message from VM 127.0.0.2
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.003200] [vm_interface/INFO] got the message [
] (size 1) from VM [127.0.0.2] to VM [127.0.0.1]
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.003200] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:227: [vm_interface/DEBUG] forwarding all the messages to SimGrid
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.003200] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:148: [vm_coordinator/DEBUG] going to time 0.003201
[nova-1.lyon.grid5000.fr:sender:(13) 0.003210] [vm_coordinator/INFO] sending [
] (size 1) from vm [127.0.0.2], to vm [127.0.0.1] (on pm [nova-2.lyon.grid5000.fr])
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.003400] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:136: [vm_coordinator/DEBUG] simulating to time 0.003600
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.003400] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:150: [vm_interface/DEBUG] asking all the VMs to go to time 0.003600 (0.003600)
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.003400] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:162: [vm_interface/DEBUG] getting the message send by the VMs
13:44:17 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:100: VSG_GOTO_DEADLINE recv
13:44:17 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
13:44:17 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:82: VSG waiting order
13:44:18 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:117: VSG_SEND_PACKET send time[s=0, us=3401] dest[127.0.0.1] message_length[1]
13:44:18 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.003400] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:183: [vm_interface/VERBOSE] getting a message from VM 127.0.0.2
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.003400] [vm_interface/INFO] got the message [
] (size 1) from VM [127.0.0.2] to VM [127.0.0.1]
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.003400] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:227: [vm_interface/DEBUG] forwarding all the messages to SimGrid
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.003400] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:148: [vm_coordinator/DEBUG] going to time 0.003401
[nova-1.lyon.grid5000.fr:sender:(14) 0.003410] [vm_coordinator/INFO] sending [
] (size 1) from vm [127.0.0.2], to vm [127.0.0.1] (on pm [nova-2.lyon.grid5000.fr])
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.003600] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:136: [vm_coordinator/DEBUG] simulating to time 0.003800
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.003600] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:150: [vm_interface/DEBUG] asking all the VMs to go to time 0.003800 (0.003800)
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.003600] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:162: [vm_interface/DEBUG] getting the message send by the VMs
13:44:18 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:100: VSG_GOTO_DEADLINE recv
13:44:18 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
13:44:18 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:82: VSG waiting order
13:44:18 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:117: VSG_SEND_PACKET send time[s=0, us=3601] dest[127.0.0.1] message_length[1]
13:44:18 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.003600] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:183: [vm_interface/VERBOSE] getting a message from VM 127.0.0.2
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.003600] [vm_interface/INFO] got the message [
] (size 1) from VM [127.0.0.2] to VM [127.0.0.1]
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.003600] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:227: [vm_interface/DEBUG] forwarding all the messages to SimGrid
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.003600] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:148: [vm_coordinator/DEBUG] going to time 0.003601
[nova-1.lyon.grid5000.fr:sender:(15) 0.003610] [vm_coordinator/INFO] sending [
] (size 1) from vm [127.0.0.2], to vm [127.0.0.1] (on pm [nova-2.lyon.grid5000.fr])
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.003800] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:136: [vm_coordinator/DEBUG] simulating to time 0.004000
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.003800] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:150: [vm_interface/DEBUG] asking all the VMs to go to time 0.004000 (0.004000)
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.003800] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:162: [vm_interface/DEBUG] getting the message send by the VMs
13:44:18 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:100: VSG_GOTO_DEADLINE recv
13:44:18 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
13:44:18 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:82: VSG waiting order
13:44:19 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:117: VSG_SEND_PACKET send time[s=0, us=3801] dest[127.0.0.1] message_length[1]
13:44:19 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.003800] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:183: [vm_interface/VERBOSE] getting a message from VM 127.0.0.2
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.003800] [vm_interface/INFO] got the message [
] (size 1) from VM [127.0.0.2] to VM [127.0.0.1]
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.003800] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:227: [vm_interface/DEBUG] forwarding all the messages to SimGrid
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.003800] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:148: [vm_coordinator/DEBUG] going to time 0.003801
[nova-1.lyon.grid5000.fr:sender:(16) 0.003810] [vm_coordinator/INFO] sending [
] (size 1) from vm [127.0.0.2], to vm [127.0.0.1] (on pm [nova-2.lyon.grid5000.fr])
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.004000] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:136: [vm_coordinator/DEBUG] simulating to time 0.004012
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.004000] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:150: [vm_interface/DEBUG] asking all the VMs to go to time 0.004012 (0.004012)
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.004000] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:162: [vm_interface/DEBUG] getting the message send by the VMs
13:44:19 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:100: VSG_GOTO_DEADLINE recv
13:44:19 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
13:44:19 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:82: VSG waiting order
13:44:19 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:117: VSG_SEND_PACKET send time[s=0, us=4001] dest[127.0.0.1] message_length[1]
13:44:19 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.004000] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:183: [vm_interface/VERBOSE] getting a message from VM 127.0.0.2
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.004000] [vm_interface/INFO] got the message [
] (size 1) from VM [127.0.0.2] to VM [127.0.0.1]
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.004000] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:227: [vm_interface/DEBUG] forwarding all the messages to SimGrid
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.004000] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:148: [vm_coordinator/DEBUG] going to time 0.004001
[nova-1.lyon.grid5000.fr:sender:(17) 0.004010] [vm_coordinator/INFO] sending [
] (size 1) from vm [127.0.0.2], to vm [127.0.0.1] (on pm [nova-2.lyon.grid5000.fr])
[nova-2.lyon.grid5000.fr:vsg_vm:(3) 0.004010] [vm_coordinator/INFO] delivering data [plop
] from vm [127.0.0.2] to vm [127.0.0.1]
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.004010] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:183: [vm_coordinator/DEBUG] delivering data [plop
] from vm [127.0.0.2] to vm [127.0.0.1]
13:44:19 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:155: VSG_DELIVER_PACKET send src[127.0.0.2] message_length[5]
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.004010] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:274: [vm_interface/VERBOSE] message from vm 127.0.0.2 delivered to vm 127.0.0.1
13:44:19 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:181: VSG_DELIVER_PACKET recv 1/2[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.004010] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:136: [vm_coordinator/DEBUG] simulating to time 0.004210

[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.004010] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:150: [vm_interface/DEBUG] asking all the VMs to go to time 0.004210 (0.004210)
13:44:19 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:187: VSG_DELIVER_PACKET recv 2/2 message_length[5]
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.004010] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:162: [vm_interface/DEBUG] getting the message send by the VMs
SINK] -- Decoded src=127.0.0.2
SINK] -- Decoded message=plop

13:44:19 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:82: VSG waiting order
13:44:19 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:100: VSG_GOTO_DEADLINE recv
13:44:19 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
13:44:19 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:82: VSG waiting order
13:44:21 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:117: VSG_SEND_PACKET send time[s=0, us=4201] dest[91.121.7.182] message_length[48]
13:44:21 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.004010] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:183: [vm_interface/VERBOSE] getting a message from VM 127.0.0.2
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.004010] [vm_interface/INFO] got the message [#] (size 48) from VM [127.0.0.2] to VM [91.121.7.182]
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.004010] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:227: [vm_interface/DEBUG] forwarding all the messages to SimGrid
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.004010] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:148: [vm_coordinator/DEBUG] going to time 0.004201
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.004201] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:163: [vm_coordinator/WARNING] the VM 127.0.0.2 tries to send a message to the unknown VM 91.121.7.182
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.004211] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:136: [vm_coordinator/DEBUG] simulating to time 0.004211
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.004211] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:150: [vm_interface/DEBUG] asking all the VMs to go to time 0.004211 (0.004211)
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.004211] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:162: [vm_interface/DEBUG] getting the message send by the VMs
13:44:21 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:100: VSG_GOTO_DEADLINE recv
13:44:21 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
13:44:21 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:82: VSG waiting order
13:45:26 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:117: VSG_SEND_PACKET send time[s=0, us=4401] dest[91.121.7.182] message_length[48]
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.004211] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:183: [vm_interface/VERBOSE] getting a message from VM 127.0.0.2
13:45:26 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.004211] [vm_interface/INFO] got the message [#] (size 48) from VM [127.0.0.2] to VM [91.121.7.182]
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.004211] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:227: [vm_interface/DEBUG] forwarding all the messages to SimGrid
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.004211] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:148: [vm_coordinator/DEBUG] going to time 0.004401
[nova-2.lyon.grid5000.fr:vsg_vm:(3) 0.004211] [vm_coordinator/INFO] delivering data [a
] from vm [127.0.0.2] to vm [127.0.0.1]
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.004401] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:163: [vm_coordinator/WARNING] the VM 127.0.0.2 tries to send a message to the unknown VM 91.121.7.182
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.004401] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:183: [vm_coordinator/DEBUG] delivering data [a
] from vm [127.0.0.2] to vm [127.0.0.1]
13:45:26 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:155: VSG_DELIVER_PACKET send src[127.0.0.2] message_length[2]
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.004401] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:274: [vm_interface/VERBOSE] message from vm 127.0.0.2 delivered to vm 127.0.0.1
13:45:26 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:181: VSG_DELIVER_PACKET recv 1/2
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.004401] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:136: [vm_coordinator/DEBUG] simulating to time 0.004412
13:45:26 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:187: [nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.004401] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:150: [vm_interface/DEBUG] asking all the VMs to go to time 0.004412 (0.004412)
VSG_DELIVER_PACKET recv 2/2 message_length[2]
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.004401] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:162: [vm_interface/DEBUG] getting the message send by the VMs
SINK] -- Decoded src=127.0.0.2
SINK] -- Decoded message=a
��
13:45:26 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:82: VSG waiting order
13:45:26 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:100: VSG_GOTO_DEADLINE recv
13:45:26 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
13:45:26 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:82: VSG waiting order
13:47:34 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:117: VSG_SEND_PACKET send time[s=0, us=4601] dest[91.121.7.182] message_length[48]
13:47:34 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.004401] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:183: [vm_interface/VERBOSE] getting a message from VM 127.0.0.2
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.004401] [vm_interface/INFO] got the message [#] (size 48) from VM [127.0.0.2] to VM [91.121.7.182]
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.004401] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:227: [vm_interface/DEBUG] forwarding all the messages to SimGrid
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.004401] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:148: [vm_coordinator/DEBUG] going to time 0.004601
[nova-2.lyon.grid5000.fr:vsg_vm:(3) 0.004412] [vm_coordinator/INFO] delivering data [hello
] from vm [127.0.0.2] to vm [127.0.0.1]
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.004601] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:163: [vm_coordinator/WARNING] the VM 127.0.0.2 tries to send a message to the unknown VM 91.121.7.182
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.004601] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:183: [vm_coordinator/DEBUG] delivering data [hello
] from vm [127.0.0.2] to vm [127.0.0.1]
13:47:34 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:155: VSG_DELIVER_PACKET send src[127.0.0.2] message_length[6]
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.004601] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:274: [vm_interface/VERBOSE] message from vm 127.0.0.2 delivered to vm 127.0.0.1
13:47:34 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:181: VSG_DELIVER_PACKET recv 1/2
13:47:34 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:187: [nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.004601] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsCoordinator.cpp:136: [vm_coordinator/DEBUG] simulating to time 0.004612
VSG_DELIVER_PACKET recv 2/2 message_length[6]
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.004601] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:150: [vm_interface/DEBUG] asking all the VMs to go to time 0.004612 (0.004612)
SINK] -- Decoded src=127.0.0.2
[nova-0.lyon.grid5000.fr:vm_coordinator:(1) 0.004601] /home/msimonin/workspace/repos/2018-vsg/src/simgrid/VmsInterface.cpp:162: [vm_interface/DEBUG] getting the message send by the VMs
SINK] -- Decoded message=hello

13:47:34 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:82: VSG waiting order
13:47:34 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:100: VSG_GOTO_DEADLINE recv
13:47:34 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:93: VSG_AT_DEADLINE send
13:47:34 DEBUG /home/msimonin/workspace/repos/2018-vsg/src/vsg/vsg.c:82: VSG waiting order
```