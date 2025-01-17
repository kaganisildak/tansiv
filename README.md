Réflexion sur le couplage entre des vraies VM qui exécutent du vrai
code et une simulation SimGrid. On veut que les applis dans les VM
aient une illusion robuste d'une exécution non modifiée. Il faut que
cette illusion persiste pour les applis qui cherchent à détecter si
elles sont observées en regardant comment le temps avance.

# Compilation

```
mkdir build
cd build
cmake -DFLATBUFFERS_SRC=<path to flatbuffers sources> -DCMAKE_INSTALL_PREFIX=/opt/tansiv ..  && make && make install
```

# Tests unitaires

Il y a des tests à différents niveau:

- code rust: `cargo test` dans les sous répertoire
    - tests unitaires de l'implémentation cliente (sans simgrid)

- code c++: `./tests`
    - tests des bindings c du code rust (le code qui sera embarqué dans les
      applis c/c++)
    - à cause de https://gitlab.inria.fr/quinson/2018-vsg/-/issues/5 on doit les lancer individuellement:
    `./tests --list-test-names-only | xargs -d "\n" -n1  ./tests`

# Tests fonctionnels

Cette fois Simgrid est impliqué.

- `send`: échange d'un message entre deux processus (utilise l'implémentation sur process tanproc)

```
./tansiv   examples/send/nova_cluster.xml examples/send/deployment.xml --log=vm_interface.threshold:debug --log=vm_coordinator.threshold:debug
```

- `qemu`: Lance des machines virtuelles dont les communications passent sur
simgrid. Il faut:
  - le programme `genisoimage` (pour générer l'iso cloud-init), `qemu-img` (pour créer les disques des VMs à la volée)
  - une image de base compatible: par exemple celle construite à l'aide de `packer`.
  - qemu compilé dans src/qemu qui contient notre backend réseau `tantap` (un backend `tap` modifié qui
    intercepte/réinjecte les communications vers/en provenance de simgrid)

```
cd examples/qemu
../../tansiv star.xml deployment_2.xml --log=vm_interface.threshold:debug --log=vm_coordinator.threshold:debug
```

# Docker

Goal: having a single line command to run
- a tansiv system (several QEMUs + simgrid)
- a non tansiv system (only QEMUs + linux bridge)
- an environment with all the deps

## Build

```
docker build -t tansiv:latest .
```

## No Tansiv

- From docker (2 VMs, this calls `boot.py` twice).

  ```bash
  docker run  --device /dev/net/tun --cap-add NET_ADMIN -v $(pwd)/tools/packer:/srv/packer -ti tansiv:latest notansiv.py --qemu_cmd qemu-system-x86_64 --qemu_mem 1g --qemu_image /srv/packer//packer-debian-11.1.0-x86_64-qemu/debian-11.1.0-x86_64.qcow2 --autoconfig_net --number 2
  ```

- On your local environment (example with a single VM)

  ```
   bin/boot.py --mode notansiv --qemu_cmd qemu-system-x86_64  --qemu_image tools/packer/packer-debian-11.1.0-x86_64-qemu/debian-11.1.0-x86_64.qcow2 --qemu_mem 1g --qemu_args "-cpu core2duo -icount shift=0,sleep=off,align=off -rtc clock=vm" mysocket 192.168.120.11/24 10.0.0.11/24
  ```

## Tansiv

```bash
docker run  --device /dev/net/tun --cap-add NET_ADMIN -v /home/msimonin/workspace/repos/2018-vsg/tools/packer/packer-debian-11.1.0-x86_64-qemu/debian-11.1.0-x86_64.qcow2:/srv/image.qcow2 -v $(pwd)/examples/qemu_docker:/srv/inputs -ti tansiv:latest tansiv /srv/inputs/star.xml /srv/inputs/deployment_2.xml --log=vm_interface.threshold:debug --log=vm_coordinator.threshold:debug
```
