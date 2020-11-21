#!/bin/sh -eux

apt-get -y update
# netperf
apt-get -y install software-properties-common
apt-add-repository non-free
apt-get -y update

apt-get -y install cloud-init iperf

apt-get -y purge libx11-data xauth libxmuu1 libxcb1 libx11-6 libxext6
apt-get -y purge ppp pppconfig pppoeconf
apt-get -y purge popularity-contest

apt-get -y autoremove
apt-get -y clean;

rm -rf /usr/share/doc/*

find /var/cache -type f -exec rm -rf {} \;
find /var/log/ -name *.log -exec rm -f {} \;

