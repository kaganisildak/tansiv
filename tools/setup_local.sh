#!/usr/bin/env bash

# virtual machines descriptors
DESCS="10 11"

for desc in $DESCS
do
  ip link show dev tantap-br || ip link add name tantap-br type bridge
  ip link set tantap-br up
  (ip addr show dev tantap-br | grep 192.168.120.1/24) || ip addr add 192.168.120.1/24 dev tantap-br
  ip link show dev tantap${desc} || ip tuntap add tantap${desc} mode tap
  ip link set tantap${desc} master tantap-br
  ip link set tantap${desc} up

  ip link show dev mantap-br || ip link add name mantap-br type bridge
  ip link set mantap-br up
  (ip addr show dev mantap-br | grep 10.0.0.1/24) || ip addr add 10.0.0.1/24 dev mantap-br
  ip link show dev mantap${desc} || ip tuntap add mantap${desc} mode tap
  ip link set mantap${desc} master mantap-br
  ip link set mantap${desc} up
done
