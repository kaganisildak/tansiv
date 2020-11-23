#!/bin/sh -eux

apt-get -y update
# netperf
apt-get -y install software-properties-common
apt-add-repository non-free
apt-get -y update

apt-get -y install cloud-init iperf mpi-default-bin wget vim

# Get the NAS parallel benchmark
mkdir -p /opt
cd /opt
wget https://www.nas.nasa.gov/assets/npb/NPB3.3.1.tar.gz
tar -xvzf NPB3.3.1.tar.gz
chown -R tansiv:tansiv .

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

