#include "VmsInterface.hpp"
#include <simgrid/s4u.hpp>


XBT_LOG_NEW_DEFAULT_CATEGORY(vm_interface, "Logging specific to the VmsInterface");

namespace vsg{

bool sortMessages(message i, message j){
  return i.time < i.time;
}

VmsInterface::VmsInterface(std::string executable_path, std::unordered_map<std::string,std::string> host_of_vms, bool stop_condition){

  vm_deployments = host_of_vms;
  a_vm_stopped = false; 
  simulate_until_any_stop = stop_condition;

  int connection_socket = socket(PF_LOCAL, SOCK_STREAM, 0);
  XBT_INFO("socket created");

  struct sockaddr_un address;
  address.sun_family = AF_LOCAL;
  strcpy(address.sun_path, CONNECTION_SOCKET_NAME);
  XBT_INFO("address of socket ready");

  if(bind(connection_socket, (sockaddr*)(&address), sizeof(address)) != 0){
    close(connection_socket);
    std::perror("unable to bind connection socket");
    exit(666);
  }
  XBT_INFO("socket binded");

  if(listen(connection_socket, 1) != 0){
    close(connection_socket);
    std::perror("unable to listen on connection socket");
    exit(666);
  }
  XBT_INFO("listen on socket");

  for(auto it : vm_deployments){
    std::string vm_name = it.first;

    switch(fork()){
      case -1:
        close(connection_socket);
        std::perror("unable to fork process");
        exit(666);
        break;
      case 0:
        close(connection_socket);
        if(execlp((executable_path+vm_name).c_str(), vm_name.c_str(), CONNECTION_SOCKET_NAME)!=0){
          std::perror("unable to launch VM");
          exit(666);
        }
        break;
      default:
        break;
    }
    XBT_INFO("fork done for VM %s",vm_name.c_str());

    struct sockaddr_un vm_address = {0};
    unsigned int len = sizeof(vm_address);
    int vm_socket = accept(connection_socket, (sockaddr*)(&vm_address), &len);
    if(vm_socket<0)
      std::perror("unable to accept connection on socket");

    vm_sockets[vm_name] = vm_socket;
    XBT_INFO("connection for VM %s established",vm_name.c_str());
  }

  close(connection_socket);
}

VmsInterface::~VmsInterface(){
  for(auto it : vm_sockets){
    close(it.second);
  }
}

bool VmsInterface::vmActive(){
  return (!vm_sockets.empty() && !simulate_until_any_stop) || (!a_vm_stopped && simulate_until_any_stop);
}

vsg_time VmsInterface::simgridToVmTime(double simgrid_time){
  struct vsg_time vm_time;

  // the simgrid time correspond to a double in second, so the number of seconds is the integer part
  vm_time.seconds = (uint64_t) (std::floor(simgrid_time));
  // and the number of usecond is the decimal number scaled accordingly
  vm_time.useconds = (uint64_t) (std::floor((simgrid_time - std::floor(simgrid_time)) * 1e6 ));

  return vm_time;
}

double VmsInterface::vmToSimgridTime(vsg_time vm_time){
  return vm_time.seconds + (vm_time.useconds * 1e-6);
}

std::vector<message> VmsInterface::goTo(double deadline){
  
  std::vector<message> messages;

  // first, we ask all the VMs to go to deadline
  XBT_INFO("asking all the VMs to go to time %f (%f)", deadline, vmToSimgridTime(simgridToVmTime(deadline)));
  uint32_t goto_flag = vsg_msg_from_actor_type::VSG_GO_TO_DEADLINE;
  struct vsg_time vm_deadline = simgridToVmTime(deadline);

  for(auto it : vm_sockets){
    send(it.second, &goto_flag, sizeof(uint32_t), 0);
    send(it.second, &vm_deadline, sizeof(vsg_time), 0);
  }
  
  // then, we pick up all the messages send by the VM until they reach the deadline
  XBT_INFO("getting the message send by the VMs");
  auto it = vm_sockets.begin();
  while(it != vm_sockets.end()){
    uint32_t vm_flag = 0;
    std::string vm_name = it->first;
    int vm_socket = it->second;

    while(true){
     
      recv(vm_socket, &vm_flag, sizeof(uint32_t), MSG_WAITALL);
     
      // we continue until the VM reach the deadline
      if(vm_flag == vsg_msg_to_actor_type::VSG_AT_DEADLINE){
        it++;
        break;
    
      }else if(vm_flag == vsg_msg_to_actor_type::VSG_END_OF_EXECUTION){
        close(vm_socket);
        it = vm_sockets.erase(it);
        a_vm_stopped = true; 
        XBT_INFO("the vm %s stopped its execution",vm_name);
        break;

      }else if(vm_flag == vsg_msg_to_actor_type::VSG_SEND_PACKET){
        
	XBT_INFO("getting a message from VM %s",vm_name.c_str());

        struct vsg_send_packet packet = {0,0};
        // we first get the message size
        recv(vm_socket, &packet, sizeof(packet), MSG_WAITALL);
        if(packet.packet.size < 16){
          std::perror("error in packet size!");
        }
        // then we get the message itself
	char dest[16];
        char data[packet.packet.size - 16];
        recv(vm_socket, dest, sizeof(dest), MSG_WAITALL);
        recv(vm_socket, data, sizeof(data), MSG_WAITALL);
        
        struct message m;
        m.packet_size = packet.packet.size - 16;
        m.data = data;
        m.src = vm_name;
        m.dest = dest;
        m.time = vmToSimgridTime(packet.send_time);
        messages.push_back(m);
	
      }else{
        std::perror("unknown message received from VM");
        exit(666);
      }
    }
  }
  XBT_INFO("sending all the message to SimGrid");

  return messages;
}

std::string VmsInterface::getHostOfVm(std::string vm_name){
  return vm_deployments[vm_name];  
}

void VmsInterface::deliverMessage(message m){
  
  XBT_INFO("delivering message from vm %s to vm %s", m.src, m.dest);
  if(vm_sockets.find(m.dest) != vm_sockets.end()){
    int socket = vm_sockets[m.dest];
    uint32_t deliver_flag = vsg_msg_to_actor_type::VSG_SEND_PACKET;
    struct vsg_packet packet = {0};
    packet.size = m.packet_size;

    send(socket, &deliver_flag, sizeof(uint32_t), 0);
    send(socket, &packet, sizeof(vsg_packet), 0);
    send(socket, m.data, packet.size, 0);

    XBT_INFO("message from vm %s delivered to vm %s", m.src, m.dest);
  }else{
    XBT_INFO("message from vm %s was not delivered to vm %s because it already stopped its execution");
  }
}

}
