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

- `send`: échange d'un message entre deux processus (utilise l'implémentation sur process de fake-vm)

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


