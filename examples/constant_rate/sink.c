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
  char *target = argv[2];
  float rate = atof(argv[3]);
  int total = atoi(argv[4]);
  double timestep = 1 / rate;
  int k = 1;
  int vsg_socket = vsg_connect();
  struct vsg_time previous_deadline = {0, 0};
  struct vsg_time next_deadline = {0, 0};
  while (true) {
    uint32_t order = -1;
    if (vsg_recv_order(vsg_socket, &order) < 0) {
      return;
    }
    struct vsg_time deadline = {0, 1};
    struct vsg_time offset = {0, 20};
    switch (order) {
    case VSG_GO_TO_DEADLINE: {
      vsg_at_deadline_recv(vsg_socket, &deadline);
      previous_deadline = next_deadline;
      next_deadline = deadline;
      // send a message if we have to (we try to keep a constant rate)
      // so painful
      double a = vsg_time_to_s(previous_deadline);
      double b = vsg_time_to_s(next_deadline);
      while (a < k * timestep && k * timestep <= b) {
        struct vsg_time send_time = vsg_time_from_s(k * timestep);
        char message[15];
        sprintf(message, "fromsink_%05d", k);
        // TODO(msimonin): handle port correctly. e.g do an echo
        struct vsg_addr dest = {inet_addr(target), 1234};
        struct vsg_addr src = {inet_addr(myself), 4321};
        struct vsg_packet packet = {
            .size = sizeof(message) + 1, .dest = dest, .src = src};
        struct vsg_send_packet send_packet = {.send_time = send_time,
                                              .packet = packet};
        vsg_send_send(vsg_socket, send_packet, message);
        k++;
        if (k > total) {
          vsg_at_deadline_send(vsg_socket);
          return 0;
        }
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
      break;
    }
    default:
      printf("SINK] error: unknown message\n");
      break;
    }
  }
  return 0;
}