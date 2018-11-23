#include "VmsInterface.hpp"
#include <simgrid/s4u.hpp>

XBT_LOG_NEW_DEFAULT_CATEGORY(vm_interface, "Logging specific to the VmsInterface");

namespace vsg{

bool sortMessages(message i, message j){
  return i.time < i.time;
}

DummyVmsInterface::DummyVmsInterface(){
  vm_names = {"vm1","vm2","vm3"};
  
  int i = 0;
  for(std::string vm_name : vm_names){
    host_of_vms[vm_name] = simgrid::s4u::Engine::get_instance()->get_all_hosts()[i]->get_name();
    i++;
  }

  dest_of_vm["vm1"] = "vm2";
  dest_of_vm["vm2"] = "vm3";
  dest_of_vm["vm3"] = "vm1";

  vms_sending_times["vm1"] = {1.0, 10.0, 30.0};
  vms_sending_times["vm2"] = {1.0, 10.2, 25.6};
  vms_sending_times["vm3"] = {1.0, 10.3};
}

DummyVmsInterface::~DummyVmsInterface(){

}

bool DummyVmsInterface::vmActive(){
  for(auto it : vms_sending_times){
    if(!it.second.empty()){
      return true;
    }
  }
  return false;
}


std::vector<message> DummyVmsInterface::goTo(double deadline){
  std::vector<message> messages;

  for(auto it : vms_sending_times){
    while(!vms_sending_times[it.first].empty() && vms_sending_times[it.first][0] <= deadline){
      vsg_packet packet;      
      packet.size = nb_packet_send;

      message m;
      m.time = vms_sending_times[it.first][0];
      m.data = packet;
      m.src = it.first;
      m.dest = dest_of_vm[it.first];
      m.packet_size = message_size;

      messages.push_back(m);
      
      vms_sending_times[m.src].erase(vms_sending_times[m.src].begin());       

      nb_packet_send++;
      XBT_INFO("vm %s send packet %i to vm %s at time %f (deadline = %f)",m.src.c_str(),m.data.size,m.dest.c_str(),m.time,deadline);
    }
  }

  std::sort(messages.begin(), messages.end(), sortMessages);

  return messages;  
}

std::string DummyVmsInterface::getHostOfVm(std::string vm_name){
  return host_of_vms[vm_name];
}

void DummyVmsInterface::deliverMessage(message m){
  XBT_INFO("vm %s received message %i",m.dest.c_str(),m.data.size);
}

}

