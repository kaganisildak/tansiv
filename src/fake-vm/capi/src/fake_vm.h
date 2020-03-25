#ifndef __FAKE_VM_H__
#define __FAKE_VM_H__

#include <stdint.h>
#include <sys/time.h>

struct vsg_context;

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
};

struct vsg_send_packet {
  struct vsg_time time;
  struct vsg_packet packet;
};

typedef void (*vsg_recv_cb)(const struct vsg_context *context, uint32_t msglen,
                            const uint8_t *msg);

struct vsg_context *vsg_init(int argc, const char *const argv[],
                             int *next_arg_p, vsg_recv_cb recv_cb);
void vsg_cleanup(struct vsg_context *context);

int vsg_start(const struct vsg_context *context);
int vsg_stop(const struct vsg_context *context);

int vsg_gettimeofday(const struct vsg_context *context, struct timeval *timeval,
                     void *timezone);
int vsg_send(const struct vsg_context *context, uint32_t msglen,
             const uint8_t *msg);

#endif /* __FAKE_VM_H__ */
