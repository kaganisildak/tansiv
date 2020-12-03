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


# Example jouets

... TEC: Travail En Cours...

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
./tansiv examples/qemus/nova_cluster.xml examples/qemus/deployment.xml --log=vm_interface.threshold:debug --log=vm_coordinator.threshold:debug
```

