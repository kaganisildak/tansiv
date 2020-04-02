#include "log.h"
#include "vsg.h"
#include <arpa/inet.h>
#include <limits.h>
#include <math.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>

struct vsg_time vsg_time_add(struct vsg_time time1, struct vsg_time time2)
{
  struct vsg_time time;
  time.seconds  = time1.seconds + time2.seconds;
  time.useconds = time1.useconds + time2.useconds;
  if (time.useconds >= 1e6) {
    time.useconds = time.useconds - 1e6;
    time.seconds++;
  }

  return time;
}

struct vsg_time vsg_time_sub(struct vsg_time time1, struct vsg_time time2)
{
  // assume to be positive values
  struct vsg_time time;
  time.seconds  = time1.seconds - time2.seconds;
  time.useconds = time1.useconds - time2.useconds;
  if (time.useconds < 0) {
    time.useconds = time.useconds + 1e6;
    time.seconds--;
  }
  return time;
}
// true if time1 <= time2
bool vsg_time_leq(struct vsg_time time1, struct vsg_time time2)
{

  if (time1.seconds < time2.seconds)
    return true;

  if ((time1.seconds == time2.seconds) && (time1.useconds <= time2.useconds))
    return true;

  return false;
}

double vsg_time_to_s(struct vsg_time time1)
{
  return time1.seconds + time1.useconds * 1e-6;
}

struct vsg_time vsg_time_from_s(double seconds)
{
  struct vsg_time time;
  time.seconds  = (uint64_t)(floor(seconds));
  time.useconds = (uint64_t)(floor((seconds - floor(seconds)) * 1e6));
  return time;
}

struct vsg_time vsg_time_cut(struct vsg_time time1, struct vsg_time time2, float a, float b)
{
  struct vsg_time time;
  double _time1 = vsg_time_to_s(time1);
  double _time2 = vsg_time_to_s(time2);
  double _time  = (a * _time1 + b * _time2) / (a + b);
  time.seconds  = (uint64_t)(floor(_time));
  time.useconds = (uint64_t)(floor((_time - floor(_time)) * 1e6));
  return time;
}

bool vsg_time_eq(struct vsg_time time1, struct vsg_time time2)
{
  return (time1.seconds * 1e6 + time1.useconds) == (time2.seconds * 1e6 + time2.useconds);
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

int vsg_deliver_send(int fd, struct vsg_deliver_packet deliver_packet, const char* message)
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
