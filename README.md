Réflexion sur le couplage entre des vraies VM qui exécutent du vrai
code et une simulation SimGrid. On veut que les applis dans les VM
aient une illusion robuste d'une exécution non modifiée. Il faut que
cette illusion persiste pour les applis qui cherchent à détecter si
elles sont observées en regardant comment le temps avance.

# Compilation

```
mkdir build
cd build
cmake .. && make
```
TODO c'est pas tout à fait ça :( (voir la chaîne de build dans le ci ou l'image docker).


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

- `qemus`: Lance des machines virtuelles dont les communications passent sur
simgrid. Il faut:
  - le programme `genisoimage` (pour générer l'iso cloud-init), `qemu-img` (pour créer les disques des VMs à la volée)
  - une image de base compatible: par exemple celle construite à l'aide de `packer`.
  - notre backend réseau `tantap` (un backend `tap` modifié qui
    intercepte/réinjecte les communications vers/en provenance de simgrid)

```
cd examples/qemus
../../tansiv nova_cluster.xml deployment.xml --log=vm_interface.threshold:debug --log=vm_coordinator.threshold:debug
```

# Docker (par exemple sur g5k)


```
g5k-setup-docker -t

cd examples/qemus

(get the debian-10.3.0-x86_64.qcow2 image)

docker run   \
  --network host \
  --device /dev/net/tun \
  --cap-add=NET_ADMIN \
  -e AUTOCONFIG_NET=1 \
  -e IMAGE=debian-10.3.0-x86_64.qcow2 \
  -v /home/msimonin/.ssh/id_rsa.pub:/root/.ssh/id_rsa.pub \
  -v $(pwd):/srv \
  -ti  registry.gitlab.inria.fr/msimonin/2018-vsg/tansiv:ad782302 nova_cluster.xml deployment.xml
```

ensuite (ça met un peu de temps à arriver)
```
# connection using the management interface should be able after some time
$) ssh -l root 172.16.0.10

# test d'un ping à travers vsg
root@tansiv-192-168-120-10:~# ping t11

PING t11 (192.168.120.11) 56(84) bytes of data.
64 bytes from t11 (192.168.120.11): icmp_seq=1 ttl=64 time=400 ms
64 bytes from t11 (192.168.120.11): icmp_seq=2 ttl=64 time=400 ms
```

# Automatiquement sur g5k

```
cd grid5000
pip install -r requirements
python g5k.py deploy ../packer/packer-debian-10.3.0-x86_64-qemu/debian-10.3.0-x86_64.qcow2 inputs/nova_cluster.xml inputs/deployment_10_on_nova.xml --cluster grvingt --queue production

python g5k.py validate

[...]
################## <mantap18> #################
mantap10 : 0.15 0.40 0.51 0.44 0.44 0.29 0.15 0.15 0.15 0.39
mantap11 : 0.17 0.15 0.18 0.26 0.23 0.30 0.19 0.15 0.17 0.37
mantap12 : 0.43 0.17 0.65 0.61 0.23 0.53 0.16 0.36 0.42 0.27
mantap13 : 0.22 0.21 0.83 0.25 0.74 0.61 0.62 0.54 0.88 0.25
mantap14 : 0.16 0.19 0.16 0.25 0.15 0.19 0.16 0.16 0.17 0.17
mantap15 : 0.16 0.29 0.28 0.26 0.15 0.32 0.15 0.15 0.20 0.15
mantap16 : 0.25 0.30 0.48 0.23 0.23 0.15 0.17 0.18 0.16 0.22
mantap17 : 0.19 0.16 0.15 0.53 0.71 0.19 0.59 0.15 0.20 0.18
mantap18 : 0.02 0.02 0.02 0.02 0.02 0.02 0.02 0.02 0.02 0.02
mantap19 : 0.16 0.16 0.26 0.17 0.16 0.25 0.39 0.16 0.17 0.16
tantap10 : 400.54 400.19 400.16 400.20 400.14 400.55 400.26 400.18 400.31 400.18
tantap11 : 400.14 400.20 400.11 400.17 400.20 400.15 400.20 400.19 400.19 400.22
tantap12 : 400.49 400.18 400.17 400.23 400.25 400.19 400.28 400.21 400.22 400.20
tantap13 : 400.10 400.17 400.23 400.15 400.17 400.21 400.27 400.13 400.18 400.18
tantap14 : 400.29 400.13 400.23 400.24 400.31 400.29 400.29 400.22 400.35 400.19
tantap15 : 400.18 400.15 400.26 400.16 400.09 400.16 400.16 400.13 400.16 400.27
tantap16 : 400.19 400.21 400.21 400.18 400.18 400.17 400.21 400.21 400.40 400.18
tantap17 : 400.25 400.12 400.15 400.15 400.22 400.19 400.25 400.15 400.17 400.19
tantap18 : 0.02 0.02 0.02 0.02 0.02 0.02 0.02 0.02 0.02 0.02
tantap19 : 400.19 400.29 400.17 400.19 400.19 400.13 400.22 400.20 400.28 400.17
[...]
```
