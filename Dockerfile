FROM registry.gitlab.inria.fr/tansiv/ns-3-dev/ns3-master


RUN apt-get update

RUN apt-get install -y git \
  g++ \
  python3 \
  cmake \
  ninja-build \
  ccache \
  tcpdump \
  libflatbuffers-dev \
  libcppunit-dev \
  libglib2.0-dev \
  cargo \
  clang \
  simgrid

RUN apt-get install -y libclang-dev \
    curl \
    git \
    pkg-config \
    ninja-build \
    libglib2.0-dev \
    libpixman-1-dev \
    libtinyxml2-dev \
    flex \
    bison \
    genisoimage \
    iproute2 \
    bzip2 \
    libvirt-dev \ 
    libjson-c-dev \ 
    libyajl-dev \   
    libxen-dev \
    python3-jinja2 \
    && rm -rf /var/lib/apt/lists/* \
    && apt-get -y autoremove \
    && apt-get -y clean \
    && find /var/cache -type f -exec rm -rf {} \; \
    && find /var/log/ -name *.log -exec rm -f {} \;

WORKDIR /app
COPY . /app
 
ENV LD_LIBRARY_PATH /ns3/build/lib:$LD_LIBRARY_PATH
WORKDIR /app/build
RUN cmake -DCMAKE_INSTALL_PREFIX=/opt/tansiv -DCMAKE_BUILD_TYPE=Release -DNS3_SUPPORT=ON -DNS3_HINT=/ns3/build .. && make && make install && make xen_tansiv_bridge && cp ../src/xen/xen_tansiv_bridge /opt/tansiv/bin
# FIXME make xen_tansiv_brige in ALL target but this will need more parameterization 
# as this assumes tansiv-client.h to be installed in /opt/tansiv

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
RUN ../../tansiv-simgrid platform.xml deployment.xml --sock_name send.sock | grep "Received from" | wc -l | grep 2
# Testing on NS3 isn't that simple since NS3 expects a true ethernet packet to be sent
# however we send some raw buffers

WORKDIR /app/build
RUN make gettimeofday
WORKDIR /app/build/examples/benchs
RUN ../../tansiv-simgrid nova_cluster.xml deployment.xml --sock_name gettimeofday.sock --force 1

## build qemu with the new network backend (tantap)
WORKDIR /app/src/qemu
RUN ./configure --target-list=x86_64-softmmu --prefix=/usr/local --extra-cflags="-I/opt/tansiv/include" --extra-ldflags="-lrt /opt/tansiv/lib/libtanqemu.a /opt/tansiv/lib/libtansiv-timer.a" && make -j  && make install && mv /usr/local/bin/qemu-system-x86_64 /usr/local/bin/tanqemu-system-x86_64
RUN ./configure --target-list=x86_64-softmmu --prefix=/usr/local --extra-cflags="-I/opt/tansiv/include" --extra-ldflags="-lrt /opt/tansiv/lib/libtanqemukvm.a /opt/tansiv/lib/libtansiv-timer.a" && make -j  && make install && mv /usr/local/bin/qemu-system-x86_64 /usr/local/bin/tanqemukvm-system-x86_64

## make some room
# RUN rm -rf /app
#
ENV PATH=/opt/tansiv/bin:$PATH

# create an ssh key (not really usefull, we'd want our local key to be pushed
# inside the vm anyway)
RUN mkdir -p /root/.ssh
RUN ssh-keygen -t rsa -P '' -f /root/.ssh/id_rsa

WORKDIR /srv
