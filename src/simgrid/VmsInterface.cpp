#include "VmsInterface.hpp"
#include <simgrid/s4u.hpp>
#include <unistd.h>

XBT_LOG_NEW_DEFAULT_CATEGORY(vm_interface, "Logging specific to the VmsInterface");

namespace vsg{

bool sortMessages(message i, message j){
  return i.sent_time < j.sent_time;
}

VmsInterface::VmsInterface(std::unordered_map<std::string,std::string> host_of_vms, bool stop_at_any_stop){

  vm_deployments = host_of_vms;
  a_vm_stopped = false; 
  simulate_until_any_stop = stop_at_any_stop;

  int connection_socket = socket(PF_LOCAL, SOCK_STREAM, 0);
   XBT_INFO("socket created");

  struct sockaddr_un address;
  address.sun_family = AF_LOCAL;
  strcpy(address.sun_path, CONNECTION_SOCKET_NAME);

  if(bind(connection_socket, (sockaddr*)(&address), sizeof(address)) != 0){
    std::perror("unable to bind connection socket");
    closeAndExit(connection_socket);
  }
  XBT_DEBUG("socket binded");

  if(listen(connection_socket, 1) != 0){
    std::perror("unable to listen on connection socket");
    closeAndExit(connection_socket);
  }
  XBT_DEBUG("listen on socket");

  for(auto it : vm_deployments){
    std::string vm_name = it.first;

    switch(fork()){
      case -1:
        close(connection_socket);
        std::perror("unable to fork process");
        closeAndExit(-1);
        break;
      case 0:
        close(connection_socket);
        for(auto it : vm_sockets){
          close(it.second);
        }
        if(execlp(("./"+vm_name).c_str(), vm_name.c_str(), CONNECTION_SOCKET_NAME)!=0){
          std::perror("unable to launch VM");
          closeAndExit(connection_socket);
        }
        break;
      default:
        break;
    }
    XBT_DEBUG("fork done for VM %s",vm_name.c_str());

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
  endSimulation();
}


void VmsInterface::endSimulation(){
  for(auto it : vm_sockets){
    close(it.second);
  }
  unlink(CONNECTION_SOCKET_NAME);
  XBT_INFO("vm sockets are down");
}

void VmsInterface::closeAndExit(int connection_socket){
   if(connection_socket>=0)
     close(connection_socket);

   endSimulation();
   exit(666);
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
        XBT_INFO("the vm %s stop its execution",vm_name.c_str());
        shutdown(vm_socket, SHUT_RDWR);
        close(vm_socket);
        it = vm_sockets.erase(it);
        a_vm_stopped = true; 
        break;

      }else if(vm_flag == vsg_msg_to_actor_type::VSG_SEND_PACKET){
        
	XBT_DEBUG("getting a message from VM %s", vm_name.c_str());
        struct vsg_send_packet packet = {0,0};
        // we first get the message size
        recv(vm_socket, &packet, sizeof(packet), MSG_WAITALL);
        // then we get the message itself and we split
        //   - the destination address (first part) that is only useful for setting up the communication in SimGrid
        //   - and the data transfer, that correspond to the data actually send through the (simulated) network
        // (nb: we use vm_name.length() to determine the size of the destination address because we assume all the vm id to have the same size)
        char dest[vm_name.length()+1];
        char data[packet.packet.size - vm_name.length()];
        if(recv(vm_socket, dest, vm_name.length(), MSG_WAITALL) <= 0){
          XBT_ERROR("can not receive the detination of the message from VM %s. The socket may be closed",vm_name.c_str());
          closeAndExit(-1);
        }
        if(recv(vm_socket, data, sizeof(data), MSG_WAITALL) <= 0){
          XBT_ERROR("can not receive the data of the message from VM %s. The socket may be closed",vm_name.c_str());
          closeAndExit(-1);
        }
        dest[vm_name.length()] = '\0';

        XBT_DEBUG("got the message [%s] (size %lu) from VM [%s] to VM [%s]",data, sizeof(data), vm_name.c_str(), dest);
        
        struct message m;
        // NB: packet_size is the size used by SimGrid to simulate the transfer of the data on the network. 
        //     It does NOT correspond to the size of the data transfered to/from the VM on the REAL socket.
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

std::string VmsInterface::getHostOfVm(std::string vm_name){
  if(vm_deployments.find(vm_name)==vm_deployments.end()){ 
    XBT_ERROR("unknown host for vm [%s] (size(%lu)) !!!", vm_name.c_str(), sizeof(vm_name));
    closeAndExit(-1);
  }
  return vm_deployments[vm_name];  
}

void VmsInterface::deliverMessage(message m){
  
  if(vm_sockets.find(m.dest) != vm_sockets.end()){
    int socket = vm_sockets[m.dest];
    uint32_t deliver_flag = vsg_msg_from_actor_type::VSG_DELIVER_PACKET;
    std::string data = m.src + m.data;
    struct vsg_packet packet = {data.length()};

    send(socket, &deliver_flag, sizeof(deliver_flag), 0);
    send(socket, &packet, sizeof(packet), 0);
    send(socket, data.c_str(), data.length(), 0);

    XBT_DEBUG("message from vm %s delivered to vm %s", m.src.c_str(), m.dest.c_str());
  }else{
    XBT_WARN("message from vm %s was not delivered to vm %s because it already stopped its execution", m.src.c_str(), m.dest.c_str());
  }
}

}
