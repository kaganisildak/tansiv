#include <vector>
#include <arpa/inet.h>
#include <limits>
#include <math.h>
#include <string>
#include <unordered_map>
#include <vector>

extern "C"
{
#include "vsg.h"
}

int max_message = 4;
struct vsg_time delay = {0, 222000};
std::vector<std::string> dest_name;
std::string myself;

int main(int argc, char *argv[])
{

  myself = std::string(argv[1]);
  for (int i = 2; i < argc; i++)
  {
    dest_name.push_back(std::string(argv[i]));
  }

  int vm_socket = vsg_connect();

  int nb_message_send = 0;
  struct vsg_time time = {0, 0};
  struct vsg_time next_message_time = {0, 0};

  while (nb_message_send < max_message)
  {

    uint32_t master_order = 0;
    //vsg_recv_order
    if (recv(vm_socket, &master_order, sizeof(uint32_t), MSG_WAITALL) <= 0)
    {
      shutdown(vm_socket, SHUT_RDWR);
      exit(666);
    }

    if (master_order == vsg_msg_from_actor_type::VSG_GO_TO_DEADLINE)
    {

      struct vsg_time deadline;
      // vsg_recv_deadline -> vsg_at_deadline_recv
      recv(vm_socket, &deadline, sizeof(vsg_time), MSG_WAITALL);

      while (vsg_time_leq(next_message_time, deadline))
      {

        int dest_id = nb_message_send % dest_name.size();
        std::string dest = dest_name[dest_id];
        std::string message = "ping_" + std::to_string(nb_message_send);
        uint32_t send_packet_flag = vsg_msg_to_actor_type::VSG_SEND_PACKET;

        // printf("sending message %s to %s\n", message.c_str(), dest.c_str());
        // we don't care about the port set let it be 0
        vsg_addr _dest = {inet_addr(dest.c_str()), 0};
        vsg_addr _src = {inet_addr(myself.c_str()), 0};
        vsg_packet packet = {
            .size = message.length(),
            .dest = _dest,
            .src = _src};
        // -> vsg_send_packet_send
        vsg_send_send(vm_socket, next_message_time, packet, message.c_str());

        nb_message_send++;
        next_message_time = vsg_time_add(next_message_time, delay);

        if (nb_message_send >= max_message)
        {
          // Bail out -- no need to warn the coordinator beforehand
          break;
        }
      }

      time = deadline;
      uint32_t at_deadline = vsg_msg_to_actor_type::VSG_AT_DEADLINE;
      // -> vsg_at_deadline_send()
      vsg_at_deadline_send(vm_socket);
    }
    else if (master_order == vsg_msg_from_actor_type::VSG_DELIVER_PACKET)
    {
      /* First receive packet metadat */
      vsg_deliver_packet deliver_packet = {0};
      // -> vsg_deliver_packet_recv
      vsg_deliver_recv_1(vm_socket, &deliver_packet);
      char message[deliver_packet.packet.size];
      // -> vsg_deliver_packet_recvfrom
      vsg_deliver_recv_2(vm_socket, message, deliver_packet.packet.size);
    }
    else
    {
      printf("error unexpected message received %i", master_order);
    }
  }

  // printf("done, see you");
  // Bail out -- the coordinator will notice on its own
  vsg_close(vm_socket);

  return 0;
}
