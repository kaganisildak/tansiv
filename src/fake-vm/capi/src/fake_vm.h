#ifndef __FAKE_VM_H__
#define __FAKE_VM_H__

#include <stdint.h>
#include <sys/time.h>

#define MAX_PACKET_SIZE 2048

struct vsg_context;

typedef void (*vsg_recv_cb)(uintptr_t recv_cb_arg);

struct vsg_context* vsg_init(int argc, const char* const argv[], int* next_arg_p, vsg_recv_cb recv_cb,
                             uintptr_t recv_cb_arg);
void vsg_cleanup(struct vsg_context* context);

int vsg_start(const struct vsg_context* context);
int vsg_stop(const struct vsg_context* context);

int vsg_gettimeofday(const struct vsg_context* context, struct timeval* timeval, void* timezone);
int vsg_send(const struct vsg_context* context, uint32_t dest, uint32_t msglen, const uint8_t* msg);
int vsg_recv(const struct vsg_context* context, uint32_t* src, uint32_t* dest, uint32_t* msglen, uint8_t* msg);
int vsg_poll(const struct vsg_context* context);

#endif /* __FAKE_VM_H__ */
