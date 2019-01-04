#include "vsg.h"
#include <vector>
#include <string>
#include <unordered_map>
#include <sys/socket.h>
#include <sys/un.h>
#include <sys/types.h>
#include <math.h>
#include <limits>

int max_message = 20;
struct vsg_time delay = {0, 111200};
std::string dest_name = "dummy_ping";

// true if time1 <= time2
bool vsg_time_leq(struct vsg_time time1, struct vsg_time time2){
  
  if(time1.seconds < time2.seconds)
    return true;
 
  if((time1.seconds == time2.seconds) && (time1.useconds <= time2.useconds))
    return true;

  return false;
}


struct vsg_time vsg_time_add(struct vsg_time time1, struct vsg_time time2){

  struct vsg_time time;
  time.seconds = time1.seconds + time2.seconds;
  time.useconds = time1.useconds + time2.useconds;
  if(time.useconds >= 1e6){
    time.useconds = time.useconds - 1e6;
    time.seconds++;
  }

  return time;
}


int main(int argc, char *argv[])
{
  int vm_socket = socket(PF_LOCAL, SOCK_STREAM, 0);  
  
  struct sockaddr_un address;
  address.sun_family = AF_LOCAL;
  strcpy(address.sun_path, argv[1]);

  if(connect(vm_socket, (sockaddr*)(&address), sizeof(address)) != 0){
    std::perror("unable to create VM socket");
    exit(666);
  }

  int nb_message_send = 0;
  struct vsg_time time;
  struct vsg_time next_message_time;

  while(nb_message_send < max_message){
    
    uint32_t master_order = 0;
    recv(vm_socket, &master_order, sizeof(uint32_t), MSG_WAITALL);
    
    if(master_order == vsg_msg_from_actor_type::VSG_GO_TO_DEADLINE){
      
      struct vsg_time deadline;
      recv(vm_socket, &deadline, sizeof(vsg_time), MSG_WAITALL);

      while(true){
        
        std::string message = dest_name + "pong_" + std::to_string(nb_message_send);
        vsg_packet packet = {sizeof(message)};
        uint32_t send_packet_flag = vsg_msg_to_actor_type::VSG_SEND_PACKET;

        send(vm_socket, &send_packet_flag, sizeof(uint32_t), 0);
        send(vm_socket, &packet, sizeof(vsg_packet), 0);
        send(vm_socket, &message, sizeof(message), 0);
        printf("sending message to dummy_ping");

        next_message_time.seconds = std::numeric_limits<uint64_t>::max();
        next_message_time.useconds =  std::numeric_limits<uint64_t>::max();
      }

      printf("at deadline");
      time = deadline;
      uint32_t at_deadline = vsg_msg_to_actor_type::VSG_AT_DEADLINE;
      send(vm_socket, &at_deadline, sizeof(uint32_t), 0);

    }else if(master_order ==  vsg_msg_from_actor_type::VSG_DELIVER_PACKET){
      
      vsg_packet packet;
      recv(vm_socket, &packet, sizeof(vsg_packet), MSG_WAITALL);
      char message[packet.size] = "";
      recv(vm_socket, message, sizeof(message), MSG_WAITALL);
      next_message_time = vsg_time_add(time, delay);
      printf("received message : %s", message); 

    }else{
      printf("error unexpected message received %i",master_order);
    }
  }

  uint32_t end_of_execution = vsg_msg_to_actor_type::VSG_END_OF_EXECUTION;
  send(vm_socket, &end_of_execution, sizeof(uint32_t), 0);
  shutdown(vm_socket,2);

  return 0;
}
