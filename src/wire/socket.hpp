#ifndef __VSG_H__
#define __VSG_H__

#include <arpa/inet.h>
#include <stdbool.h>
#include <stdint.h>

#include <packets_generated.h>

/*
 * Low-level functions
 * Send and receive full messages even if interrupted by signals
 *
 * @return 0 on success, -1 on failure with errno set accordingly (errno == EPIPE on EOF)
 */
int vsg_protocol_send(int, const void*, size_t);
int vsg_protocol_recv(int, void*, size_t);

/*
 * Helper function to read from a socket a size prefixed flatbuffer message
 *
 * @return follows vsg_protocol_recv semantics
 *         0 on success, -1 on failure with errno set accordingly
 *         (errno == ENOBUFS if a not enough buffer is passed)
*/
int fb_recv(int sock, uint8_t* buffer, size_t buf_size);

#endif