#include "VmsInterface.hpp"
#include <simgrid/s4u.hpp>


XBT_LOG_NEW_DEFAULT_CATEGORY(vm_interface, "Logging specific to the VmsInterface");

namespace vsg{

bool sortMessages(message i, message j){
  return i.time < i.time;
}

VmsInterface::VmsInterface(std::vector<std::string> vm_names){

  for(std::string vm_name : vm_names){

    int vm_socket = socket(PF_LOCAL, SOCK_STREAM, 0); 
    
    struct sockaddr_un address;
    address.sun_family = AF_LOCAL; 
    address.sun_path = strncpy(address.sun_path, vm_name.c_str(), sizeof(address.sun_path));     

    if(bind(vm_socket, (sockaddr*)(&address), sizeof(address)) != 0)
      throw "unable to create socket for vm ";

    if(listen(vm_socket, 1) != 0)
      throw "can not initiate connection with socket of vm";

    vm_sockets[vm_name] = vm_socket;
  }
}

VmsInterface::~VmsInterface(){

}

bool VmsInterface::vmActive(){

}


std::vector<message> VmsInterface::goTo(double deadline){

}

std::string VmsInterface::getHostOfVm(std::string vm_name){

}

void VmsInterface::deliverMessage(message m){

}

}
