# Compilation

1. compiler `tansiv`
2. compiler `qemu` modifié avec slirp modifié

3. lancer tansiv en positionant le chemin vers celui-ci:
```
export QEMU=/home/msimonin/workspace/repos/qemu/build/x86_64-softmmu/qemu-system-x86_64
./tansiv ../platform/nova_cluster.xml ../examples/qemu_sink/deployment.xml --log=vm_interface.threshold:debug --log=vm_coordinator.threshold:debug
```
