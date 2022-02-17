# tansiv in docker :)
FROM simgrid/unstable:latest

WORKDIR /app
COPY . /app

RUN apt-get update
RUN apt-get install -y build-essential \
    libboost-dev \
    cmake \
    libcppunit-dev \
    libglib2.0-dev \
    cargo \
    clang \
    libclang-dev \
    curl \
    git \
    pkg-config \
    ninja-build \
    libglib2.0-dev \
    libpixman-1-dev \
    flex \
    bison \
    genisoimage \
    iproute2

RUN cargo --help

WORKDIR /app/build
RUN cmake -DCMAKE_INSTALL_PREFIX=/opt/tansiv .. && make && make install

# Outside of Rust tests, Rust panics are bugs
ENV RUST_BACKTRACE=1
# This will run the tansiv tests and the client tests
# nocapture allows for displaying log message of children actor in the tansiv-client tests
# see https://gitlab.inria.fr/tansiv/tansiv/-/merge_requests/23
RUN TEST_FLAGS="--nocapture" make run-tests

# run some functionnals ...
WORKDIR /app/build
RUN make send
WORKDIR /app/build/examples/send
RUN ../../tansiv nova_cluster.xml deployment.xml --sock_name send.sock | grep "Received from"

WORKDIR /app/build
RUN make gettimeofday
WORKDIR /app/build/examples/benchs
RUN ../../tansiv nova_cluster.xml deployment.xml --sock_name gettimeofday.sock --force 1

# build qemu with the new network backend (tantap)
WORKDIR /app/src/qemu
RUN ./configure --target-list=x86_64-softmmu  --extra-cflags="-I/opt/tansiv/include" --extra-ldflags="/opt/tansiv/lib/libtanqemu.a" && make -j  && make install

# make some room
# RUN rm -rf /app

ENV PATH=/opt/tansiv/bin:$PATH

# create an ssh key (not really usefull, we'd want our local key to be pushed
# inside the vm anyway)
RUN mkdir -p /root/.ssh
RUN ssh-keygen -t rsa -P '' -f /root/.ssh/id_rsa

WORKDIR /srv
