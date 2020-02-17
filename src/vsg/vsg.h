#ifndef __VSG_H__
#define __VSG_H__

#include <arpa/inet.h>
#include <stdint.h>
#include <stdbool.h>

#define CONNECTION_SOCKET_NAME "simgrid_connection_socket"

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

struct vsg_packet {
  uint32_t size;
};

/* Message bodies */

struct vsg_deliver_packet {
  struct vsg_packet packet;
};

struct vsg_go_to_deadline {
  struct vsg_time deadline;
};

/* struct vsg_at_deadline { */
/* }; */

struct vsg_send_packet {
  struct vsg_time send_time;
  struct vsg_packet packet;
};

/* Message type tags */

/* Sent as uint32_t */
enum vsg_msg_from_actor_type {
  VSG_DELIVER_PACKET,
  VSG_GO_TO_DEADLINE,
};

/* Sent as uint32_t */
enum vsg_msg_to_actor_type { VSG_AT_DEADLINE, VSG_SEND_PACKET };

/*
 *
 * Some util functions mostly extracted from the first examples (e.g,
 * DummyPing/Pong...)
 *
 */

struct vsg_time vsg_time_add(struct vsg_time, struct vsg_time);

bool vsg_time_leq(struct vsg_time, struct vsg_time);

/*
 *
 * Some functions to handle the vsg protocol
 *
 */
int vsg_init(void);

int vsg_connect(void);

int vsg_close(int);

int vsg_shutdown(int);

int vsg_send(int, struct vsg_time, struct in_addr, const char*, int);

int vsg_deliver_send(int, struct in_addr, const char*, int);

int vsg_send_at_deadline(int);

int vsg_recv_order(int, uint32_t*);

int vsg_recv_deadline(int, struct vsg_time*);

int vsg_deliver_recv_1(int fd, struct vsg_packet*);

int vsg_deliver_recv_2(int, char*, int, struct in_addr*);

#endif /* __VSG_H__ */