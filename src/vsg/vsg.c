#include "vsg.h"
#include <stdio.h>
#include <stdlib.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>
#include <stdbool.h>

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

// true if time1 <= time2
bool vsg_time_leq(struct vsg_time time1, struct vsg_time time2)
{

  if (time1.seconds < time2.seconds)
    return true;

  if ((time1.seconds == time2.seconds) && (time1.useconds <= time2.useconds))
    return true;

  return false;
}


int vsg_connect(void)
{
  // TODO(msimonin): use a logger
  printf("VSG] CONNECT_TO_SVG\n");
  int vm_socket = socket(PF_LOCAL, SOCK_STREAM, 1);

  struct sockaddr_un address;
  address.sun_family = AF_LOCAL;
  strcpy(address.sun_path, CONNECTION_SOCKET_NAME);

  if (connect(vm_socket, (struct sockaddr*)(&address), sizeof(address)) != 0) {
    printf("VSG] ERROR CONNECTING TO %s\n", CONNECTION_SOCKET_NAME);
    return -1;
  }
  printf("VSG] CONNECTION SUCCESSFUL\n");
  return vm_socket;
}

int vsg_close(int fd)
{
  close(fd);
}

int vsg_shutdown(int fd)
{
  shutdown(fd, SHUT_RDWR);
}

int vsg_send(int fd, struct vsg_time time, const char* dest, int dest_length, const char* message, int message_length)
{
  /* Prepend the destination to the payload */
  char vsg_payload[dest_length + message_length];
  sprintf(vsg_payload, "%s%s", dest, message);
  int vsg_payload_size = dest_length + message_length;
  struct vsg_send_packet packet = {time, {vsg_payload_size}};
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

  /*send vsg_payload*/
  ret = send(fd, vsg_payload, vsg_payload_size, 0);
  if (ret < 0)
    return -1;
  return 0;
}

int vsg_send_at_deadline(int fd)
{
  enum vsg_msg_to_actor_type at_deadline = VSG_AT_DEADLINE;
  return send(fd, &at_deadline, sizeof(at_deadline), 0);
}

int vsg_recv_order(int fd, uint32_t *order)
{
  return recv(fd, order, sizeof(uint32_t), MSG_WAITALL);
}

int vsg_recv_deadline(int fd, struct vsg_time *deadline)
{
  int ret = recv(fd, deadline, sizeof(struct vsg_time), MSG_WAITALL);
  // TODO(msimonin): this can be verbose, I really need to add a logger
  // printf("VSG] -- deadline = %d.%d\n", deadline->seconds, deadline->useconds);
  return ret;
}

int vsg_recv_packet(int fd, struct vsg_packet *packet)
{
  return recv(fd, packet, sizeof(struct vsg_packet), MSG_WAITALL);
}

int vsg_recv_payload(int fd, char* dest, int dest_size, char* message, int message_size)
{
  // TODO(msimonin): handle errors
  recv(fd, dest, dest_size, MSG_WAITALL);
  dest[dest_size] = '\0';
  recv(fd, message, message_size, MSG_WAITALL);
  message[message_size] = '\0';
}