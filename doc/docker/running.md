To run TANSIV with the `docker` backend, you will need to put `tandocker` and its arguments in a deployment file.
An example of this can be found in `examples/docker`.

After this, you should be able to run `tansiv` with the wanted network and deployment files.

You should add `--cfg=network/model:CM02 --cfg=network/TCP-gamma:0` to `tansiv` for the latencies to be consistent with the network description.


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
