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
  char *myself = argv[1];
  int vsg_socket = vsg_connect();
  struct vsg_time previous_deadline;
  struct vsg_time next_deadline;
  uint64_t _id = 0;
  while (1) {
    _id++;
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
        char message[15];
        sprintf(message, "fromsink_%05d", _id);
        // send this somewhere
        // the addr_in is important since the coordinator will route according
        // to this the port is random currently: we dispatch inside qemu
        // according to the src port
        struct vsg_addr dest = {inet_addr("127.0.0.1"), htons(4321)};
        // addr and port where the server (sink) is listening
        // there is actually no server running this but we mimic the
        // corresponding behaviour
        struct vsg_addr src = {inet_addr(myself), htons(1234)};
        struct vsg_packet packet = {
            .size = sizeof(message) + 1, .dest = dest, .src = src};
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