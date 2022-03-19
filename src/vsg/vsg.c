#include "vsg.h"
#include "log.h"
#include <arpa/inet.h>
#include <errno.h>
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

int vsg_protocol_send(int fd, const void *buf, size_t len)
{
  size_t curlen = len;
  ssize_t count;

  do {
    count = send(fd, buf + (len - curlen), curlen, 0);
    if (count > 0) {
      curlen -= count;
    } else if (count < 0 && errno != EINTR) {
      return -1;
    }
  } while (curlen > 0);

  return 0;
}

int vsg_protocol_recv(int fd, void *buf, size_t len)
{
  size_t curlen = len;
  ssize_t count;

  do {
    count = recv(fd, buf + (len - curlen), curlen, MSG_WAITALL);
    if (count > 0) {
      curlen -= count;
    } else if (count == 0 && curlen > 0) {
      /* The peer closed the socket prematurately. */
      errno = EPIPE;
      return -1;
    } else if (count < 0 && errno != EINTR) {
      return -1;
    }
  } while (curlen > 0);

  return 0;
}
