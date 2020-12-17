#!/bin/sh -eux

apt -y update

# some utils
# apt-get -y install software-properties-common
# apt-add-repository non-free
apt-get -y update

apt -y install cloud-init wget vim git autotools-dev automake texinfo

# ok we'll need netperf which isn't in buster/non-free !
# let's get it the old way
git clone https://github.com/HewlettPackard/netperf.git
cd netperf
./autogen.sh
./configure --enable-demo --prefix /usr/local
make && make install

# some apps to validate the bouzin
# since we have ssh ready on the management interface, we could also install
# some apps on the fly
apt -y install taktuk fping iperf flent python3-matplotlib python3-setuptools

mkdir -p /home/tansiv/.ssh
ssh-keygen -t rsa -f /home/tansiv/.ssh/id_rsa -P ''
cat /home/tansiv/.ssh/id_rsa.pub >> /home/tansiv/.ssh/authorized_keys
chown tansiv:tansiv -R /home/tansiv/.ssh

apt-get -y purge libx11-data xauth libxmuu1 libxcb1 libx11-6 libxext6
apt-get -y purge ppp pppconfig pppoeconf
apt-get -y purge popularity-contest

apt-get -y autoremove
apt-get -y clean;

rm -rf /usr/share/doc/*

find /var/cache -type f -exec rm -rf {} \;
find /var/log/ -name *.log -exec rm -f {} \;

