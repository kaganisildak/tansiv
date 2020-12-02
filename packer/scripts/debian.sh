#!/bin/sh -eux

apt -y update

# some utils
apt -y install cloud-init iperf wget vim

# some apps to validate the bouzin
# since we have ssh ready on the management interface, we could also install
# some apps on the fly
apt -y install taktuk

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

