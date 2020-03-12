#include "vsg.h"
#include "log.h"
#include <arpa/inet.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>
#include <stdbool.h>
#include <limits.h>
#include <math.h>

struct vsg_time vsg_time_add(struct vsg_time time1, struct vsg_time time2)
{
  struct vsg_time time;
  time.seconds = time1.seconds + time2.seconds;
  time.useconds = time1.useconds + time2.useconds;
  if (time.useconds >= 1e6)
  {
    time.useconds = time.useconds - 1e6;
    time.seconds++;
  }

  return time;
}

struct vsg_time vsg_time_sub(struct vsg_time time1, struct vsg_time time2)
{
  // assume to be positive values
  struct vsg_time time;
  time.seconds = time1.seconds - time2.seconds;
  time.useconds = time1.useconds - time2.useconds;
  if (time.useconds < 0)
  {
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

int vsg_init(void)
{
  char *log_level = getenv("VSG_LOG");
  int level = LOG_INFO;
  if (log_level != NULL)
    level = atoi(log_level);
  log_set_level(level);
  log_info("Welcome to VSG");
}

int vsg_connect(void)
{
  vsg_init();
  log_debug("Create an UNIX socket to %s", CONNECTION_SOCKET_NAME);
  int vm_socket = socket(PF_LOCAL, SOCK_STREAM, 1);

  struct sockaddr_un address;
  address.sun_family = AF_LOCAL;
  strcpy(address.sun_path, CONNECTION_SOCKET_NAME);

  if (connect(vm_socket, (struct sockaddr *)(&address), sizeof(address)) != 0)
  {
    log_error("We've got a problem connecting to the UNIX socket %s", CONNECTION_SOCKET_NAME);
    return -1;
  }
  log_debug("vsg connection established [fd=%d]", vm_socket);
  return vm_socket;
}

int vsg_close(int fd)
{
  log_debug("Closing the underlyling socket [fd=%d]", fd);
  close(fd);
}

int vsg_shutdown(int fd)
{
  log_debug("Shutting down the underlyling socket [fd=%d]", fd);
  shutdown(fd, SHUT_RDWR);
}

int vsg_recv_order(int fd, uint32_t *order)
{
  log_debug("VSG waiting order");
  return recv(fd, order, sizeof(uint32_t), MSG_WAITALL);
}

/*
 * VSG_AT_DEADLINE related functions
 */

int vsg_at_deadline_send(int fd)
{
  log_debug("VSG_AT_DEADLINE send");
  enum vsg_msg_to_actor_type at_deadline = VSG_AT_DEADLINE;
  return send(fd, &at_deadline, sizeof(at_deadline), 0);
}

int vsg_at_deadline_recv(int fd, struct vsg_time *deadline)
{
  log_debug("VSG_GOTO_DEADLINE recv");
  int ret = recv(fd, deadline, sizeof(struct vsg_time), MSG_WAITALL);
  // TODO(msimonin): this can be verbose, I really need to add a logger
  // printf("VSG] -- deadline = %d.%d\n", deadline->seconds, deadline->useconds);
  return ret;
}

/*
 * VSG_SEND_PACKET related functions
 */

int vsg_send_send(int fd, struct vsg_time time, struct vsg_dest dest, const char *message, int message_length)
{
  struct in_addr dest_addr = {dest.addr};
  log_debug("VSG_SEND_PACKET send time[s=%ld, us=%ld] dest[%s] message_length[%d]",
            time.seconds,
            time.useconds,
            inet_ntoa(dest_addr),
            message_length);

  struct vsg_send_packet packet = {time, {message_length, dest}};
  enum vsg_msg_to_actor_type send_packet_flag = VSG_SEND_PACKET;
  int ret = 0;

  /* send the send flag*/
  ret = send(fd, &send_packet_flag, sizeof(send_packet_flag), 0);
  if (ret < 0)
    return -1;

  /*send packet info*/
  ret = send(fd, &packet, sizeof(packet), 0);
  if (ret < 0)
    return -1;

  /*send the message*/
  ret = send(fd, message, message_length, 0);
  if (ret < 0)
    return -1;
  return 0;
}

/*
 * VSG_DELIVER_PACKET related functions
 */

int vsg_deliver_send(int fd, struct vsg_dest dest, const char *message, int message_length)
{
  struct in_addr dest_addr = {dest.addr};
  log_debug("VSG_DELIVER_PACKET send src[%s] message_length[%d]", inet_ntoa(dest_addr), message_length);
  enum vsg_msg_from_actor_type deliver_flag = VSG_DELIVER_PACKET;
  struct vsg_deliver_packet packet = {message_length, dest};
  int ret = 0;
  ret = send(fd, &deliver_flag, sizeof(deliver_flag), 0);
  if (ret < 0)
    return -1;

  ret = send(fd, &packet, sizeof(packet), 0);
  if (ret < 0)
    return -1;

  ret = send(fd, message, message_length, 0);
  if (ret < 0)
    return -1;
  return 0;
}

int vsg_deliver_recv_1(int fd, struct vsg_packet *packet)
{
  log_debug("VSG_DELIVER_PACKET recv 1/2");
  return recv(fd, packet, sizeof(struct vsg_packet), MSG_WAITALL);
}

int vsg_deliver_recv_2(int fd, char *message, int message_length)
{
  log_debug("VSG_DELIVER_PACKET recv 2/2 message_length[%d]", message_length);
  //printf("-- src=%s", inet_ntoa(*src));
  recv(fd, message, message_length, MSG_WAITALL);
}