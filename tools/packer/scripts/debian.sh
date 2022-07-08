#!/bin/sh -eux

apt -y update

# some utils
apt-get -y install software-properties-common
apt-add-repository non-free
apt-get -y update

apt -y install cloud-init wget vim git autotools-dev automake texinfo tmux

# some apps to validate the bouzin
# since we have ssh ready on the management interface, we could also install
# some apps on the fly
apt -y install taktuk \
    fping \
    netperf \
    iperf \
    flent \
    python3-matplotlib \
    python3-setuptools \
    redis-server

# linpack
package='l_mklb_p_2018.3.011.tgz'
mkdir /opt/linpack
cd /opt/linpack && wget "http://registrationcenter-download.intel.com/akdlm/irc_nas/9752/$package" && sha256sum -c "$package".sha256 && tar xf "$package"

# coremark
cd /opt && git clone https://github.com/eembc/coremark.git

mkdir -p /home/tansiv/.ssh
ssh-keygen -t rsa -f /home/tansiv/.ssh/id_rsa -P ''
cat /home/tansiv/.ssh/id_rsa.pub >> /home/tansiv/.ssh/authorized_keys
chown tansiv:tansiv -R /home/tansiv/.ssh

# boot fast (we're in icount mode most lickely so make the boot faster)
sed -i -e 's/^GRUB_TIMEOUT=[0-9]\+$/GRUB_TIMEOUT=0/' /etc/default/grub
update-grub


apt-get -y purge libx11-data xauth libxmuu1 libxcb1 libx11-6 libxext6
apt-get -y purge ppp pppconfig pppoeconf
apt-get -y purge popularity-contest

apt-get -y autoremove
apt-get -y clean;

rm -rf /usr/share/doc/*

find /var/cache -type f -exec rm -rf {} \;
find /var/log/ -name *.log -exec rm -f {} \;

