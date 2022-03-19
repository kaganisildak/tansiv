#ifndef __VSG_H__
#define __VSG_H__

#include <arpa/inet.h>
#include <stdbool.h>
#include <stdint.h>

void dump_packet(const uint8_t*, size_t);

void vsg_pg_port(in_port_t, uint8_t*, int, uint8_t*);
void vsg_upg_port(void*, int, in_port_t*, uint8_t**);

/*
 * Low-level functions
 * Send and receive full messages even if interrupted by signals
 *
 * @return 0 on success, -1 on failure with errno set accordingly (errno == EPIPE on EOF)
 */
int vsg_protocol_send(int, const void*, size_t);
int vsg_protocol_recv(int, void*, size_t);

#endif
