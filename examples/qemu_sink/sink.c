#include "vsg.h"
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
    switch(order)
    {
      case VSG_GO_TO_DEADLINE:
      {
        vsg_recv_deadline(vsg_socket, &deadline);
        /* Don't do anything here.
          -- this yields to the qemu process until it declares the same
        */
        vsg_send_at_deadline(vsg_socket);
        break;
      }
      case VSG_DELIVER_PACKET:
      {
        /* First receive the size of the payload. */
        struct vsg_packet packet = {0};
        vsg_recv_packet(vsg_socket, &packet);

        /* Second get the vsg payload = src + message. */
        int message_size = packet.size - sizeof(struct in_addr);
        char message[message_size];
        struct in_addr src = {0};
        vsg_recvfrom_payload(vsg_socket, message, message_size, &src);

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