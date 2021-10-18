#ifndef __VSG_H__
#define __VSG_H__

#include <arpa/inet.h>
#include <stdbool.h>
#include <stdint.h>

#define CONNECTION_SOCKET_NAME "/tmp/simgrid_connection_socket"

/* Messages types */
/*
 * Communication over the network:
 * - use little-endian encoding
 * - sequence:
 *   1. msg type tag (4 bytes)
 *   2. msg body (sizeof(struct vsg_*) bytes, can be empty, eg. vsg_at_deadline)
 *   3. (for messages containing vsg_packet) application packet data (vsg_packet.size bytes)
 * - in local communications (eg. UNIX sockets), Step 3 can be implemented with
 *   shared memory
 */

/* Common types in message bodies */

struct vsg_time {
  uint64_t seconds;
  uint64_t useconds;
};

enum vsg_msg_in_type {
  DeliverPacket,
  GoToDeadline,
  EndSimulation,
};

enum vsg_msg_out_type {
  AtDeadline,
  SendPacket,
};
struct vsg_packet {
  uint32_t size;
  in_addr_t src;
  in_addr_t dst;
};

struct vsg_send_packet {
  struct vsg_time send_time;
  struct vsg_packet packet;
};

struct vsg_deliver_packet {
  struct vsg_packet packet;
};

void dump_packet(const uint8_t*, size_t);

void vsg_pg_port(in_port_t, uint8_t*, int, uint8_t*);
void vsg_upg_port(void*, int, in_port_t*, uint8_t**);

/*
 * Decoding function
 */
int vsg_decode_src_dst(struct vsg_send_packet, char* src_addr, char* dst_addr);

/*
 * Receive order from vsg
 */
int vsg_recv_order(int, uint32_t*);

/*
 * VSG_AT_DEADLINE related functions
 */

int vsg_at_deadline_recv(int, struct vsg_time*);

int vsg_at_deadline_send(int);

/*
 * VSG_DELIVER_PACKET related functions
 */

// TODO(msimonin): why don't we have time here ?
int vsg_deliver_send(int, struct vsg_deliver_packet, const uint8_t*);

/*
 * Low-level functions
 * Send and receive full messages even if interrupted by signals
 *
 * @return 0 on success, -1 on failure with errno set accordingly (errno == EPIPE on EOF)
 */
int vsg_protocol_send(int fd, const void *buf, size_t len);
int vsg_protocol_recv(int fd, void *buf, size_t len);

#endif
