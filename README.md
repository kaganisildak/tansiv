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


# Example jouets

... TEC: Travail En Cours...

- `dummy_ping/pong`: Implementation d'échange de messages au niveau du
protocole vsg. Les messages transitent par simgrid qui gère le temps de
propagation en fonction du fichier de plateform. Le déploiement
  - 1 processus ping: envoie un ping (à travers la socket vsg) aux deux processus pong
  - 2 prorcessus pong: lorsqu'ils reçoivent le ping, décodent la source
  (entête du protocole vsg) et renvoie un pong.


- `qemu_sink`: illustre la modification de SLIRP (backend réseau de QEMU) pour faire
 transiter les messages sortants (UDP seulement pour l'instant) à travers
 vsg.

- `constant_rate`: 2 processes s'envoie des messages à vitesse constantes
(mesurée en nombre de messages par seconde). C'est pratique pour avoir un
comportement déterministe dans les échanges des messages.