# tansiv in docker :)
FROM simgrid/unstable:latest

WORKDIR /app
COPY . /app

RUN apt-get update
# putting all in one layer make us hit
# a timeout limit when pushing the layers
RUN apt-get install -y build-essential \
    gcc-11 \
    libboost-dev \
    cmake \
    libcppunit-dev \
    libglib2.0-dev \
    cargo \
    clang
RUN libclang-dev \
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

# clone some version of flatbuffer
# flatc will be compiled from this
# and used in the rust part (build.rs) and the cpp part to compile the protocol
# IDL
RUN git clone https://github.com/google/flatbuffers --depth 1 -b v2.0.0

WORKDIR /app/build
RUN cmake -DFLATBUFFERS_SRC=/app/flatbuffers -DCMAKE_INSTALL_PREFIX=/opt/tansiv .. && make && make install

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
RUN ../../tansiv nova_cluster.xml deployment.xml --sock_name send.sock | grep "Received from" | wc -l | grep 2

WORKDIR /app/build
RUN make gettimeofday
WORKDIR /app/build/examples/benchs
RUN ../../tansiv nova_cluster.xml deployment.xml --sock_name gettimeofday.sock --force 1

# build qemu with the new network backend (tantap)
WORKDIR /app/src/qemu
RUN ./configure --cc=/usr/bin/gcc-11 --target-list=x86_64-softmmu  --extra-cflags="-I/opt/tansiv/include" --extra-ldflags="/opt/tansiv/lib/libtanqemu.a" && make -j  && make install

# make some room
# RUN rm -rf /app

ENV PATH=/opt/tansiv/bin:$PATH

# create an ssh key (not really usefull, we'd want our local key to be pushed
# inside the vm anyway)
RUN mkdir -p /root/.ssh
RUN ssh-keygen -t rsa -P '' -f /root/.ssh/id_rsa

WORKDIR /srv
