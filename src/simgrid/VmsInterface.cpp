#include "VmsInterface.hpp"
#include <simgrid/s4u.hpp>


XBT_LOG_NEW_DEFAULT_CATEGORY(vm_interface, "Logging specific to the VmsInterface");

namespace vsg{

bool sortMessages(message i, message j){
  return i.sent_time < j.sent_time;
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
    std::perror("unable to bind connection socket");
    closeAndExit(connection_socket);
  }
  XBT_INFO("socket binded");

  if(listen(connection_socket, 1) != 0){
    std::perror("unable to listen on connection socket");
    closeAndExit(connection_socket);
  }
  XBT_INFO("listen on socket");

  for(auto it : vm_deployments){
    std::string vm_name = it.first;

    switch(fork()){
      case -1:
        std::perror("unable to fork process");
        closeAndExit(-1);
        break;
      case 0:
        close(connection_socket);
        for(auto it : vm_sockets){
          close(it.second);
        }
        if(execlp((executable_path+vm_name).c_str(), vm_name.c_str(), CONNECTION_SOCKET_NAME)!=0){
          std::perror("unable to launch VM");
          closeAndExit(-1);
        }
        exit(0);
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
    XBT_INFO("connection for VM %s established", vm_name.c_str());
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
  XBT_DEBUG("asking all the VMs to go to time %f (%f)", deadline, vmToSimgridTime(simgridToVmTime(deadline)));
  uint32_t goto_flag = vsg_msg_from_actor_type::VSG_GO_TO_DEADLINE;
  struct vsg_time vm_deadline = simgridToVmTime(deadline);

  for(auto it : vm_sockets){
    send(it.second, &goto_flag, sizeof(uint32_t), 0);
    send(it.second, &vm_deadline, sizeof(vsg_time), 0);
  }
  
  // then, we pick up all the messages send by the VM until they reach the deadline
  XBT_DEBUG("getting the message send by the VMs");
  auto it = vm_sockets.begin();
  while(it != vm_sockets.end()){
    uint32_t vm_flag = 0;
    std::string vm_name = it->first;
    int vm_socket = it->second;

    while(true){
     
      if(recv(vm_socket, &vm_flag, sizeof(uint32_t), MSG_WAITALL) <= 0){
        XBT_ERROR("can not receive the flags of VM %s. The socket may be closed",vm_name.c_str());
        closeAndExit(-1);
      }
     
      // we continue until the VM reach the deadline
      if(vm_flag == vsg_msg_to_actor_type::VSG_AT_DEADLINE){
        it++;
        break;
    
      }else if(vm_flag == vsg_msg_to_actor_type::VSG_END_OF_EXECUTION){
        XBT_INFO("the vm %s stopped its execution",vm_name.c_str());
        close(vm_socket);
        it = vm_sockets.erase(it);
        a_vm_stopped = true; 
        XBT_INFO("vm %s socket removed",vm_name.c_str());
        break;

      }else if(vm_flag == vsg_msg_to_actor_type::VSG_SEND_PACKET){
        
	XBT_INFO("getting a message from VM %s",vm_name.c_str());

        struct vsg_send_packet packet = {0,0};
        // we first get the message size
        recv(vm_socket, &packet, sizeof(packet), MSG_WAITALL);

        // then we get the message itself (nb: we use vm_name.length() because we assume all the vm id to have the same size)
	char dest[vm_name.length()+1];
        char data[packet.packet.size];
        XBT_INFO("(dest size = %lu, message size = %lu)", vm_name.length(), packet.packet.size);
        if(recv(vm_socket, dest, vm_name.length(), MSG_WAITALL) <= 0){
          XBT_ERROR("can not receive the detination of the message from VM %s. The socket may be closed",vm_name.c_str());
          closeAndExit(-1);
        }
        if(recv(vm_socket, data, sizeof(data), MSG_WAITALL) <= 0){
          XBT_ERROR("can not receive the data of the message from VM %s. The socket may be closed",vm_name.c_str());
          closeAndExit(-1);
        }
        XBT_INFO("dest is [%s] (size=%lu), message is [%s]",dest, sizeof(dest), data);
        
        struct message m;
        m.packet_size = sizeof(data);
        m.data.append(data);
        m.src = vm_name;
        m.dest.append(dest);
        m.sent_time = vmToSimgridTime(packet.send_time);
        messages.push_back(m);
	
      }else{
        XBT_ERROR("unknown message received from VM %s : %lu",vm_name.c_str(), vm_flag);
        closeAndExit(-1);
      }
    }
  }
  XBT_DEBUG("forwarding all the message to SimGrid");

  std::sort(messages.begin(), messages.end(), sortMessages);

  return messages;
}

void VmsInterface::closeAndExit(int connection_socket){
   if(connection_socket>=0)
     close(connection_socket);
  
   for(auto it : vm_sockets){
     close(it.second);
   }

   exit(666);
}

std::string VmsInterface::getHostOfVm(std::string vm_name){
  if(vm_deployments.find(vm_name)==vm_deployments.end()){
    for(auto it : vm_deployments){
       XBT_INFO("host of vm [%s] is [%s] (size = %lu)", it.first.c_str(), it.second.c_str(), sizeof(it.first));
    }
    XBT_ERROR("unknown host for vm [%s] (size(%lu)) !!!", vm_name.c_str(), sizeof(vm_name));
    closeAndExit(-1);
  }
  return vm_deployments[vm_name];  
}

void VmsInterface::deliverMessage(message m){
  
  XBT_INFO("delivering message %s of size %i from vm %s to vm %s", m.data.c_str(), m.packet_size, m.src.c_str(), m.dest.c_str());
  if(vm_sockets.find(m.dest) != vm_sockets.end()){
    int socket = vm_sockets[m.dest];
    uint32_t deliver_flag = vsg_msg_from_actor_type::VSG_DELIVER_PACKET;
    struct vsg_packet packet = {m.packet_size};

    send(socket, &deliver_flag, sizeof(deliver_flag), 0);
    send(socket, &packet, sizeof(packet), 0);
    send(socket, m.data.c_str(), packet.size, 0);

    XBT_INFO("message from vm %s delivered to vm %s", m.src.c_str(), m.dest.c_str());
  }else{
    XBT_INFO("message from vm %s was not delivered to vm %s because it already stopped its execution", m.src.c_str(), m.dest.c_str());
  }
}

}
