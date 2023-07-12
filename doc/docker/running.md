To run TANSIV with the `docker` backend, you will need to put `tandocker` and its arguments in a deployment file.
An example of this can be found in `examples/docker`.

After this, you should be able to run `tansiv` with the wanted network and deployment files.
(The order of arguments can be determined by running `tansiv` with no arguments.)

You should add `--cfg=network/model:CM02 --cfg=network/TCP-gamma:0` to `tansiv`’s arguments for the latencies to be consistent with the network description.


## Making sure `tandocker` can find the necessary files

`tandocker` will need to have access to the `LD_PRELOAD` shim, the container stopper program, and the `docker.py` helper.

These paths are all defined as constants in `src/client/tansiv-client/src/docker/mod.rs` (all the ones ending in `_path`).

They are relative to the directory you run `tansiv` in.

As of writing this, they aren’t very well designed or consistent.
It seems in the case of QEMU, relative paths were chosen so that `tansiv` would be run from `examples/<somefolder>`.
We should make them consistent with this, which will require modifying `CMakeLists.txt` to install built files in the correct relative paths.

For now, building the `LD_PRELOAD` shim and container stopper manually and symlinking the paths so that they are consistent with the constants in `tandocker` works.


## Creating an image with the necessary tools

Default docker images are designed to be minimal, so won’t includes the tools we want.

We need to find a base image using `glibc`. <!-- TODO: do we? -->
This is the case of the `debian` image.

To create a new image from the `debian` image, we can run a container using `docker run -it debian`.
This will start a shell in a new container, with access to the internet.

 - To add software from `apt`, run `apt update`, then `apt install`.
   After you’re done, `apt clean` can be used to reduce the size of the image a bit.
 - `docker cp` allows you to copy anything from your system to the container.
   For example, if you wanted to install `darkhttpd`, which is not currently in the Debian repos,
   you could compile it outside of the container and copy the binary inside.

Once your container has all the necessary tools, exit the shell.
The container should now have disappeared from `docker ps`, but not `docker ps -a`.
Use `docker commit` to create an image from the container’s filesystem.


Some useful tools include `iproute2`, `iputils-ping`, `iperf3`, `curl` or `wget` and a web server.


## Select IP addresses for the containers

Docker won’t allow multiple “docker networks” on the same subnet.
As each container has its own tap interface, and therefore its own docker network, this means we need one subnet per container.

It’s possible to use `/30` netmasks: this will fit the network address (the two least significant bits are 0), the broadcast address (the two least significant bits are 1), the default gateway address (01), and the container address (10)
So available addresses are ones ending in `.a/30`, where *a* is 4*x*+2, for *x* an integer.


## Running experiments

When running `tansiv`, with the deployment files running `tandocker`s, containers will be started and time-controlled, but won’t run any useful programs.

`docker exec` can be used to run programs in these existing containers.
For example, `docker exec -it <container ID> /bin/bash` can run a `bash` shell.

The container IDs can be obtained from `docker ps`, they will appear in the reverse order of starting the containers (which follows the order of appearance in the deployment file).

Note that the `LD_PRELOAD` shim has been placed in `/tansiv-preload.so` in all containers, but won’t be used automatically.
You will need to define the environment variable correctly, for example using `export LD_PRELOAD=/tansiv-preload.so` will use it for the next programs run in the current shell (note that this will not include builtins such as `time`), or `env LD_PRELOAD=/tansiv-preload.so <program>` for a single program.
It is possible to do `docker exec -it /usr/bin/env LD_PRELOAD=/tansiv-preload.so bash` or similar, or run programs from `docker exec` without using a shell.
This means that programs can be run from a script, after getting the list of containers from, e.g. `containers=( $(docker ps -q) )` in `bash`.


## Cleaning up

If you interrupt `tansiv`, it won’t currently clean up what it created.
This means you can’t run `tansiv` again as it won’t try to replace what is already present.

Here are the steps to follow to ensure that no side effects remain (some steps aren’t always necessary):
 - Stop the containers started by tansiv using `docker stop <container IDs…>`.
   To get a list of running containers, run `docker ps`.
    - In the case of a crash, you might need to find the container’s cgroup directory and `echo 0 > cgroup.freeze` (or `echo 0 | sudo tee cgroup.freeze` to run as root) before the container is actually stopped
 - Ensure no `tansiv` or `tandocker` process remains (this should not normally happen)
    - `killall tandocker` can be used
 - Remove `/tmp/simgrid_connection_socket`, if still present.
 - Remove the containers started by tansiv with `docker rm <container IDs…>`.
   To get a list of running and stopped containers, run `docker ps -a`
   Alternatively, use `docker container prune` to remove all your containers (this can have side-effects if you used docker for other reasons)
 - Remove the docker networks created by tansiv using `docker network rm <network names…>`.
   They should start with `tansiv-`
   To get a list of docker networks, run `docker network ls`.
 - Remove the tap interfaces created by tansiv using `ip link del <interface name>`.
   The docker networks had the names of the interfaces as a suffix in their name.
   Their naming scheme is `taptN`, with N being a number (which can span multiple digits)
   A list can be obtained with `ip link`.
 - Remove stray `/dev/shm/tansiv-time-*` files, if necessary

For example, for a two container experiment, I used this ugly cleanup script:
```sh
#!/bin/sh
set -x
docker stop $(docker ps -q)
killall tandocker
rm -f /tmp/simgrid_connection_socket
docker container prune
docker network rm tansiv-tapt0 tansiv-tapt1
ip l del tapt0
ip l del tapt1
rm -f /dev/shm/tansiv-time-*
```
