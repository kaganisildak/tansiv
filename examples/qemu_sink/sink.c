#include "log.h"
#include "vsg.h"
#include <stdio.h>
#include <stdlib.h>

/*
 *
 * Mimic a sink
 * - first version is a sink for the vsg protocol not
 *   an UDP sink adapted for the vsg protocol
 *
 *
 */

int main(int argc, char *argv[]) {
  int vsg_socket = vsg_connect();
  struct vsg_time previous_deadline;
  struct vsg_time next_deadline;
  while (1) {
    uint32_t order;
    // TODO(msimonin): we need to define the src header length in the protocol
    // at some point.
    int dest_size = 3;
    // printf("SINK] Waiting coordinator order\n");
    vsg_recv_order(vsg_socket, &order);
    struct vsg_time deadline = {0, 1};
    struct vsg_time offset = {0, 20};
    switch (order) {
    case VSG_GO_TO_DEADLINE: {
      vsg_at_deadline_recv(vsg_socket, &deadline);
      previous_deadline = next_deadline;
      next_deadline = deadline;
      /* Don't do anything here.
          -- this yields to the qemu process until it declares the same
        */
      printf("SINK] -- deadline received=%ld.%06ld\n", deadline.seconds,
             deadline.useconds);
      // send some message
      // we send message only if there some time [previous_deadline,
      // next_deadline] isn't empty
      if (!vsg_time_eq(previous_deadline, next_deadline)) {
        const char *message = "fromsink";
        // TODO(msimonin): handle port correctly. e.g do an echo
        struct vsg_addr dest = {inet_addr("127.0.0.2"), 1234};
        struct vsg_addr src = {inet_addr("127.0.0.1"), 4321};
        struct vsg_packet packet = {
            .size = sizeof(message), .dest = dest, .src = src};
        struct vsg_send_packet send_packet = {.send_time = next_deadline,
                                              .packet = packet};
        vsg_send_send(vsg_socket, send_packet, message);
      }
      // --
      vsg_at_deadline_send(vsg_socket);
      break;
    }
    case VSG_DELIVER_PACKET: {
      /* First receive the size of the payload. */
      struct vsg_packet packet = {0};
      vsg_deliver_recv_1(vsg_socket, &packet);

      /* Second get the vsg payload = src + message. */
      char message[packet.size];
      vsg_deliver_recv_2(vsg_socket, message, packet.size);

      struct in_addr addr = {packet.dest.addr};
      printf("SINK] -- Decoded dest=%s\n", inet_ntoa(addr));
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