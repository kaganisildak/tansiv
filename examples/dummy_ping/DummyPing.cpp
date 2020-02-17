#include <vector>
#include <arpa/inet.h>
#include <limits>
#include <math.h>
#include <string>
#include <unordered_map>
#include <vector>

extern "C" {
    #include "vsg.h"
}

int max_message       = 4;
struct vsg_time delay = {0, 222000};
std::vector<std::string> dest_name;

int main(int argc, char* argv[])
{

  for (int i = 1; i < argc; i++) {
    dest_name.push_back(std::string(argv[i]));
  }

  int vm_socket = vsg_connect();

  int nb_message_send               = 0;
  struct vsg_time time              = {0, 0};
  struct vsg_time next_message_time = {0, 0};

  while (nb_message_send < max_message) {

    uint32_t master_order = 0;
    //vsg_recv_order
    if (recv(vm_socket, &master_order, sizeof(uint32_t), MSG_WAITALL) <= 0) {
      shutdown(vm_socket, SHUT_RDWR);
      exit(666);
    }

    if (master_order == vsg_msg_from_actor_type::VSG_GO_TO_DEADLINE) {

      struct vsg_time deadline;
      // vsg_recv_deadline -> vsg_at_deadline_recv
      recv(vm_socket, &deadline, sizeof(vsg_time), MSG_WAITALL);

      while (vsg_time_leq(next_message_time, deadline)) {

        int dest_id               = nb_message_send % dest_name.size();
        std::string dest          = dest_name[dest_id];
        std::string message       = "ping_" + std::to_string(nb_message_send);
        vsg_send_packet packet    = {next_message_time, message.length()};
        uint32_t send_packet_flag = vsg_msg_to_actor_type::VSG_SEND_PACKET;

        // printf("sending message %s to %s\n", message.c_str(), dest.c_str());
        in_addr_t dest_addr = inet_addr(dest.c_str());
        struct in_addr _dest_addr = {dest_addr};
        // -> vsg_send_packet_send
        vsg_send_send(vm_socket, next_message_time, _dest_addr, message.c_str(), message.length());

        nb_message_send++;
        next_message_time = vsg_time_add(next_message_time, delay);

        if (nb_message_send >= max_message) {
          // Bail out -- no need to warn the coordinator beforehand
          break;
        }
      }

      time                 = deadline;
      uint32_t at_deadline = vsg_msg_to_actor_type::VSG_AT_DEADLINE;
      // -> vsg_at_deadline_send()
      vsg_at_deadline_send(vm_socket);

    } else if (master_order == vsg_msg_from_actor_type::VSG_DELIVER_PACKET) {
      /* First receive the size of the payload. */
      vsg_packet packet = {0};
      // -> vsg_deliver_packet_recv
      vsg_deliver_recv_1(vm_socket, &packet);
      /* Second get the vsg payload = src + message. */
      int message_size = packet.size - sizeof(struct in_addr);
      char message[message_size];
      struct in_addr src = {0};
      // -> vsg_deliver_packet_recvfrom
      vsg_deliver_recv_2(vm_socket, message, message_size, &src);
    } else {
      printf("error unexpected message received %i", master_order);
    }
  }

  // printf("done, see you");
  // Bail out -- the coordinator will notice on its own
  vsg_close(vm_socket);

  return 0;
}
