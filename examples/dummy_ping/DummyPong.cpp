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

struct vsg_time delay = {0, 11200};
std::string dest_name = "";
std::string myself;
int max_message = 2;

int main(int argc, char *argv[])
{
  int vm_socket = vsg_connect();
  myself = std::string(argv[1]);
  int nb_message_send = 0;
  struct vsg_time time = {0, 0};
  struct vsg_time next_message_time = {std::numeric_limits<uint64_t>::max(), std::numeric_limits<uint64_t>::max()};

  while (nb_message_send < max_message)
  {

    uint32_t master_order = 0;
    if (vsg_recv_order(vm_socket, &master_order) <= 0)
    {
      vsg_shutdown(vm_socket);
      exit(666);
    }

    if (master_order == vsg_msg_from_actor_type::VSG_GO_TO_DEADLINE)
    {

      struct vsg_time deadline = {0, 0};
      vsg_at_deadline_recv(vm_socket, &deadline);

      while (vsg_time_leq(next_message_time, deadline))
      {

        std::string message = "pong_" + std::to_string(nb_message_send);
        uint32_t send_packet_flag = vsg_msg_to_actor_type::VSG_SEND_PACKET;

        // printf("sending message to dummy_ping");

        // we don't care about the port set let it be 0
        vsg_addr _dest = {inet_addr(dest_name.c_str()), 0};
        vsg_addr _src = {inet_addr(myself.c_str()), 0};
        vsg_packet packet = {
            .size = message.length(),
            .dest = _dest,
            .src = _src};
        vsg_send_send(vm_socket, next_message_time, packet, message.c_str());

        nb_message_send++;

        next_message_time.seconds = std::numeric_limits<uint64_t>::max();
        next_message_time.useconds = std::numeric_limits<uint64_t>::max();

        if (nb_message_send >= max_message)
        {
          // Bail out -- no need to warn the coordinator beforehand
          break;
        }
      }

      time = deadline;
      uint32_t at_deadline = vsg_msg_to_actor_type::VSG_AT_DEADLINE;
      vsg_at_deadline_send(vm_socket);
    }
    else if (master_order == vsg_msg_from_actor_type::VSG_DELIVER_PACKET)
    {
      /* First receive the size of the payload. */
      vsg_deliver_packet deliver_packet = {0};
      vsg_deliver_recv_1(vm_socket, &deliver_packet);

      /* Second get the vsg payload = src + message. */
      char message[deliver_packet.packet.size];
      struct in_addr src = {0};
      vsg_deliver_recv_2(vm_socket, message, deliver_packet.packet.size);
      dest_name = "";
      struct in_addr src_addr = {deliver_packet.packet.src.addr};
      dest_name.append(inet_ntoa(src_addr));

      next_message_time = vsg_time_add(time, delay);
    }
    else
    {
      printf("error unexpected message received %i", master_order);
    }
  }
  // printf("done, see you");
  vsg_close(vm_socket);

  return 0;
}
