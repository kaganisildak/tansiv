#include "vsg.h"
#include "log.h"
#include <arpa/inet.h>
#include <limits.h>
#include <math.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>

void dump_packet(const uint8_t* buf, size_t size)
{
  printf("Dumping packet at %p size %ld \n", buf, size);
  // c compatible dump
  printf("{");
  for (int i = 0; i < size - 1; i++) {
    printf("0x%02x,", buf[i]);
  }
  printf("0x%02x}", buf[size - 1]);
  printf("\n");
}

/**
 *
 * Debug purpose only,
 * This decode the src and dest field from in_addr to cghar*
 *
 */
int vsg_decode_src_dst(struct vsg_send_packet packet, char* src_addr, char* dst_addr)
{
  struct in_addr _dst_addr = {packet.packet.dst};
  struct in_addr _src_addr = {packet.packet.src};
  inet_ntop(AF_INET, &(_src_addr), src_addr, INET_ADDRSTRLEN);
  inet_ntop(AF_INET, &(_dst_addr), dst_addr, INET_ADDRSTRLEN);
  return 0;
}

/*
 *
 * Piggyback the port in the payload
 *
 */
void vsg_pg_port(in_port_t port, uint8_t* message, int msg_len, uint8_t* payload)
{
  uint8_t _ports[2] = {(uint8_t)(port >> 8), (uint8_t)(port & 0xff)};
  int offset        = 2 * sizeof(uint8_t);
  memcpy(payload, _ports, offset);
  memcpy(payload + offset, message, msg_len);
}

/*
 *
 * Un-Piggyback the port from the payload
 *
 */
void vsg_upg_port(void* buf, int length, in_port_t* port, uint8_t** payload)
{
  uint8_t* _buf = (uint8_t*)buf;
  uint16_t h    = ((uint16_t)*_buf) << 8;
  uint16_t l    = (uint16_t) * (_buf + 1);
  *port         = h + l;
  *payload      = (_buf + 2);
}

/*
 * VSG_AT_DEADLINE related functions
 */

int vsg_at_deadline_send(int fd)
{
  log_debug("VSG_AT_DEADLINE send");
  enum vsg_msg_out_type at_deadline = AtDeadline;
  return send(fd, &at_deadline, sizeof(at_deadline), 0);
}

int vsg_at_deadline_recv(int fd, struct vsg_time* deadline)
{
  log_debug("VSG_GOTO_DEADLINE recv");
  int ret = recv(fd, deadline, sizeof(struct vsg_time), MSG_WAITALL);
  // TODO(msimonin): this can be verbose, I really need to add a logger
  // printf("VSG] -- deadline = %d.%d\n", deadline->seconds,
  // deadline->useconds);
  return ret;
}

/*
 * VSG_DELIVER_PACKET related functions
 */

int vsg_deliver_send(int fd, struct vsg_deliver_packet deliver_packet, const uint8_t* message)
{
  // log_deliver_packet(deliver_packet);
  struct vsg_packet packet          = deliver_packet.packet;
  enum vsg_msg_in_type deliver_flag = DeliverPacket;
  int ret                           = 0;
  ret                               = send(fd, &deliver_flag, sizeof(deliver_flag), 0);
  if (ret < 0)
    return -1;

  ret = send(fd, &deliver_packet, sizeof(deliver_packet), 0);
  if (ret < 0)
    return -1;

  ret = send(fd, message, packet.size, 0);
  if (ret < 0)
    return -1;
  return 0;
}
