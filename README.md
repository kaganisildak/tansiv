Réflexion sur le couplage entre des vraies VM qui exécutent du vrai
code et une simulation SimGrid. On veut que les applis dans les VM
aient une illusion robuste d'une exécution non modifiée. Il faut que
cette illusion persiste pour les applis qui cherchent à détecter si
elles sont observées en regardant comment le temps avance.

# Run CI jobs locally

Requirements: [gitlab-runner](https://docs.gitlab.com/runner/#install-gitlab-runner)

- `tansiv`: build tansiv and qemu
  ```
  gitlab-runner exec docker tansiv --docker-volumes $(pwd)/opt:/opt
  ```

- `dummy_ping`: ping/pong between two processes with Simgrid in the middle
  ```
  gitlab-runner exec docker dummy_ping --docker-volumes $(pwd)/opt:/opt
  ```