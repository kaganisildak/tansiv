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

#include "socket.hpp"

int vsg_protocol_send(int fd, const void* buf, size_t len)
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

int vsg_protocol_recv(int fd, void* buf, size_t len)
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

int fb_recv(int sock, uint8_t* buffer, size_t buf_size)
{
  uint8_t len_buf[4];
  int ret = vsg_protocol_recv(sock, len_buf, 4);
  if (ret) {
    return ret;
  }
  auto len = flatbuffers::ReadScalar<uint32_t>(len_buf);
  if (buf_size < len) {
    errno = ENOBUFS;
    fprintf(stderr, "  %zd bytes provided but at least %d bytes required\n", buf_size, len);
    return -1;
  }
  // read the remaining part
  ret = vsg_protocol_recv(sock, buffer, len);
  return ret;
}
