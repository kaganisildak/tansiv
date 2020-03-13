#include "vsg.h"
#include "log.h"
#include <stdlib.h>
#include <stdio.h>

/*
 *
 * Mimic a sink
 * - first version is a sink for the vsg protocol not
 *   an UDP sink adapted for the vsg protocol
 *
 *
 */

int main(int argc, char *argv[])
{
  int vsg_socket = vsg_connect();
  while (1)
  {
    uint32_t order;
    // TODO(msimonin): we need to define the src header length in the protocol
    // at some point.
    int dest_size = 3;
    //printf("SINK] Waiting coordinator order\n");
    vsg_recv_order(vsg_socket, &order);
    struct vsg_time deadline = {0, 1};
    struct vsg_time offset = {0, 1};
    switch(order)
    {
      case VSG_GO_TO_DEADLINE:
      {
        vsg_at_deadline_recv(vsg_socket, &deadline);
        /* Don't do anything here.
          -- this yields to the qemu process until it declares the same
        */
        printf("SINK] -- deadline received=%ld.%06ld\n", deadline.seconds, deadline.useconds);
        // send some messages
        const char* message  = "fromsink";
        struct vsg_time time = vsg_time_sub(deadline, offset);
        struct in_addr dest  = {inet_addr("127.0.0.2")};

        vsg_send_send(vsg_socket, time, dest, message, sizeof(message));

        // --
        vsg_at_deadline_send(vsg_socket);
        break;
      }
      case VSG_DELIVER_PACKET:
      {
        /* First receive the size of the payload. */
        struct vsg_packet packet = {0};
        vsg_deliver_recv_1(vsg_socket, &packet);

        /* Second get the vsg payload = src + message. */
        int message_size = packet.size - sizeof(struct in_addr);
        char message[message_size];
        struct in_addr src = {0};
        vsg_deliver_recv_2(vsg_socket, message, message_size, &src);

        printf("SINK] -- Decoded src=%s\n", inet_ntoa(src));
        printf("SINK] -- Decoded message=%s\n", message);
        break;
      }
      default:
        printf("SINK] error: unknown message\n");
        break;
    }
  }
  return 0;
}