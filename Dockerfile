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
    libglib2.0-dev \
    libpixman-1-dev \
    flex \
    bison \
    genisoimage \
    iproute2

RUN cargo --help

WORKDIR /app/build
RUN cmake .. && make && make DESTDIR=/opt/tansiv install

# run some tests about the rust part
# WORKDIR /app/src/fake-vm
# RUN make && make test

# Outside of Rust tests, Rust panics are bugs
ENV RUST_BACKTRACE=1

# run some tests about the c/c++ part
WORKDIR /app/build
RUN ./tests --list-test-names-only | xargs -d "\n" -n1  ./tests

# build qemu with the new network backend (tantap)
WORKDIR /app/src/qemu
RUN ./configure --target-list=x86_64-softmmu && make -j  && make install

# make some room
RUN rm -rf /app

ENV PATH=/opt/tansiv/usr/local/bin:$PATH
ENV QEMU=/usr/local/bin/qemu-system-x86_64

# create an ssh key (not really usefull, we'd want our local key to be pushed
# inside the vm anyway)
RUN mkdir -p /root/.ssh
RUN ssh-keygen -t rsa -P '' -f /root/.ssh/id_rsa

WORKDIR /srv

ENTRYPOINT ["tansiv"]
