#ifndef __VSG_H__
#define __VSG_H__

#include <stdint.h>

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

struct vsg_init_time {
	struct vsg_time origin;
};

struct vsg_deliver_packet {
	struct vsg_packet packet;
};

struct vsg_go_to_deadline {
	struct vsg_time deadline;
};

/* struct vsg_at_deadline { */
/* }; */

struct vsg_send_packet {
	struct vsg_packet packet;
};

/* Message type tags */

/* Sent as uint32_t */
enum vsg_msg_from_actor_type {
	VSG_INIT_TIME,
	VSG_DELIVER_PACKET,
	VSG_GO_TO_DEADLINE,
};

/* Sent as uint32_t */
enum vsg_msg_to_actor_type {
	VSG_AT_DEADLINE,
	VSG_SEND_PACKET,
};

#endif /* __VSG_H__ */
