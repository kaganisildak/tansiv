#include <limits>
#include <math.h>
#include <string>
#include <unordered_map>
#include <vector>

extern "C" {
    #include "vsg.h"
}

struct vsg_time delay = {0, 11200};
std::string dest_name = "";
int max_message       = 2;


int main(int argc, char* argv[])
{
  int dest_size = std::atoi(argv[1]);

  int vm_socket = vsg_connect();

  int nb_message_send               = 0;
  struct vsg_time time              = {0, 0};
  struct vsg_time next_message_time = {std::numeric_limits<uint64_t>::max(), std::numeric_limits<uint64_t>::max()};

  while (nb_message_send < max_message) {

    uint32_t master_order = 0;
    if (vsg_recv_order(vm_socket, &master_order) <= 0) {
      vsg_shutdown(vm_socket);
      exit(666);
    }

    if (master_order == vsg_msg_from_actor_type::VSG_GO_TO_DEADLINE) {

      struct vsg_time deadline = {0, 0};
      vsg_recv_deadline(vm_socket, &deadline);

      while (vsg_time_leq(next_message_time, deadline)) {

        std::string message       = dest_name + "pong_" + std::to_string(nb_message_send);
        vsg_send_packet packet    = {next_message_time, message.length()};
        uint32_t send_packet_flag = vsg_msg_to_actor_type::VSG_SEND_PACKET;

        // printf("sending message to dummy_ping");
        vsg_send(vm_socket, next_message_time, message.c_str(), message.length());

        nb_message_send++;

        next_message_time.seconds  = std::numeric_limits<uint64_t>::max();
        next_message_time.useconds = std::numeric_limits<uint64_t>::max();

        if (nb_message_send >= max_message) {
          // Bail out -- no need to warn the coordinator beforehand
          break;
        }
      }

      time                 = deadline;
      uint32_t at_deadline = vsg_msg_to_actor_type::VSG_AT_DEADLINE;
      vsg_send_at_deadline(vm_socket);

    } else if (master_order == vsg_msg_from_actor_type::VSG_DELIVER_PACKET) {
      /* First receive the size of the payload. */
      vsg_packet packet = {0};
      vsg_recv_packet(vm_socket, &packet);

      /* Second receive the payload.
       *
       * The initial implementation called two subsequent recv
       * with the benefit of no data copy.
       *
       */
      char buffer[packet.size];
      vsg_recv_payload(vm_socket, buffer, packet.size);

      // This is called dest here because we send a pong message
      // in the vsg protocol the app payload is prefixed with the src
      char dest[dest_size + 1];
      std::copy(&buffer[0], &buffer[dest_size], dest);
      dest[dest_size] = '\0';
      dest_name = "";
      dest_name.append(dest);
      printf("--Decoded src=%s\n", dest);

      char message[packet.size - dest_size + 1];
      std::copy(&buffer[dest_size], &buffer[packet.size], message);
      message[packet.size - dest_size] = '\0';
      printf("--Decoded message=%s\n", message);

      next_message_time = vsg_time_add(time, delay);

    } else {
      printf("error unexpected message received %i", master_order);
    }
  }

  // printf("done, see you");
  vsg_close(vm_socket);

  return 0;
}
