You will need to build `tansiv`, which can be done using the already existing CMake infrastructure.
Adding `-DCMAKE_INSTALL_PREFIX=<path to some folder where tansiv files will be put>` to `cmake`’s arguments will allow you to build as a non-root user, as the building seems to install some files as well.

`tandocker` is an executable built from `tansiv-client`, instead of a library.
To rebuild `tandocker`, without rebuilding anything else, you can go to the `src/client` directory and run `make tandocker`.
You can add `RELEASE=<anything>` to `make`’s command line to compile in release mode.
When using `make tandocker`, executables will be placed in `target/(debug|release)/tandocker` depending on compilation target.

The `LD_PRELOAD` shim and the container stopper have `Makefile`s, that respect the `CC` and `CFLAGS` variables, if given.
