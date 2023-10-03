# tansiv in docker :)
FROM registry.gitlab.inria.fr/tansiv/ns-3-dev/ns3-master

RUN apt-get update
# putting all in one layer make us hit
# a timeout limit when pushing the layers
RUN apt-get install -y build-essential \
    libboost-dev \
    cmake \
    libflatbuffers-dev \
    libcppunit-dev \
    libglib2.0-dev \
    cargo \
    clang \
    && rm -rf /var/lib/apt/lists/* \
    && apt-get -y autoremove \
    && apt-get -y clean \
    && find /var/cache -type f -exec rm -rf {} \; \
    && find /var/log/ -name *.log -exec rm -f {} \;

RUN apt-get install -y libclang-dev \
    curl \
    git \
    pkg-config \
    ninja-build \
    libglib2.0-dev \
    libpixman-1-dev \
    flex \
    bison \
    genisoimage \
    iproute2 \
    && rm -rf /var/lib/apt/lists/* \
    && apt-get -y autoremove \
    && apt-get -y clean \
    && find /var/cache -type f -exec rm -rf {} \; \
    && find /var/log/ -name *.log -exec rm -f {} \;


RUN cargo --help

WORKDIR /app
COPY . /app

WORKDIR /app/build
RUN cmake -DCMAKE_INSTALL_PREFIX=/opt/tansiv -DNS3_HINT=/ns3/build .. && make && make install

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
RUN ./configure --target-list=x86_64-softmmu --prefix=/usr/local --extra-cflags="-I/opt/tansiv/include" --extra-ldflags="-lrt /opt/tansiv/lib/libtanqemu.a /opt/tansiv/lib/libtansiv-timer.a" && make -j  && make install && mv /usr/local/bin/qemu-system-x86_64 /usr/local/bin/tanqemu-system-x86_64
RUN ./configure --target-list=x86_64-softmmu --prefix=/usr/local --extra-cflags="-I/opt/tansiv/include" --extra-ldflags="-lrt /opt/tansiv/lib/libtanqemukvm.a /opt/tansiv/lib/libtansiv-timer.a" && make -j  && make install && mv /usr/local/bin/qemu-system-x86_64 /usr/local/bin/tanqemukvm-system-x86_64

# make some room
# RUN rm -rf /app

ENV PATH=/opt/tansiv/bin:$PATH

# create an ssh key (not really usefull, we'd want our local key to be pushed
# inside the vm anyway)
RUN mkdir -p /root/.ssh
RUN ssh-keygen -t rsa -P '' -f /root/.ssh/id_rsa

WORKDIR /srv
