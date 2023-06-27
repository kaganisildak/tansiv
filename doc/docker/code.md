Most of the additional code for `tandocker` is in `tap/`, `docker/`, and `timer/docker/`.
The rest of the necessary code is `bin/docker.py`, the `LD_PRELOAD` shim, and the container stopper.
These last two are in separate repositories (included as submodules)
