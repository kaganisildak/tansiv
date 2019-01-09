#include "vsg.h"
#include <vector>
#include <string>
#include <unordered_map>
#include <sys/socket.h>
#include <sys/un.h>
#include <sys/types.h>
#include <math.h>
#include <unistd.h>

int max_message = 4;
struct vsg_time delay = {0, 222000};
std::vector<std::string> dest_name = {"dummy_pong000001","dummy_pong000002"};

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

double vmToSimgridTime(vsg_time vm_time){
  return vm_time.seconds + (vm_time.useconds * 1e-6);
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
  struct vsg_time time = {0,0};
  struct vsg_time next_message_time = {0,0};

  while(nb_message_send < max_message){
    
    uint32_t master_order = 0;
    if(recv(vm_socket, &master_order, sizeof(uint32_t), MSG_WAITALL) <= 0){
      shutdown(vm_socket, SHUT_RDWR);
      exit(666);
    }
    
    if(master_order == vsg_msg_from_actor_type::VSG_GO_TO_DEADLINE){
      
      struct vsg_time deadline;
      recv(vm_socket, &deadline, sizeof(vsg_time), MSG_WAITALL);

      while(vsg_time_leq(next_message_time, deadline)){
          
        int dest_id = nb_message_send % dest_name.size();
        std::string dest = dest_name[dest_id];
        std::string message = dest + "ping_" + std::to_string(nb_message_send);
        vsg_send_packet packet = {next_message_time, message.length()};
        uint32_t send_packet_flag = vsg_msg_to_actor_type::VSG_SEND_PACKET;

        //printf("sending message %s to %s", message.c_str(), dest.c_str());
        send(vm_socket, &send_packet_flag, sizeof(send_packet_flag), 0);
        send(vm_socket, &packet, sizeof(packet), 0);
        //send(vm_socket, dest.c_str(), dest.length(), 0);
        send(vm_socket, message.c_str(), message.length(), 0);

        nb_message_send ++;
        next_message_time = vsg_time_add(next_message_time, delay);

        if(nb_message_send >= max_message){
          uint32_t end_of_execution = vsg_msg_to_actor_type::VSG_END_OF_EXECUTION;
          send(vm_socket, &end_of_execution, sizeof(end_of_execution), 0);
          break;
        }
      }

      time = deadline;
      uint32_t at_deadline = vsg_msg_to_actor_type::VSG_AT_DEADLINE;
      send(vm_socket, &at_deadline, sizeof(uint32_t), 0);

    }else if(master_order ==  vsg_msg_from_actor_type::VSG_DELIVER_PACKET){
      
      vsg_packet packet = {0};
      recv(vm_socket, &packet, sizeof(vsg_packet), MSG_WAITALL);
      char message[packet.size] = "";
      recv(vm_socket, message, sizeof(message), MSG_WAITALL);
      //printf("dummy_ping received message : %s", message); 

    }else{
      printf("error unexpected message received %i",master_order);
    }
  }

  //printf("done, see you");
  uint32_t end_of_execution = vsg_msg_to_actor_type::VSG_END_OF_EXECUTION;
  send(vm_socket, &end_of_execution, sizeof(end_of_execution), 0);
  
  close(vm_socket);

  return 0;
}
