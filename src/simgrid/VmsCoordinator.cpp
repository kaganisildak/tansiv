#include "simgrid/s4u.hpp"
#include "VmsInterface.hpp"
#include "vsg.h"
#include <simgrid/kernel/resource/Model.hpp>
#include <limits.h>

XBT_LOG_NEW_DEFAULT_CATEGORY(vm_coordinator, "Logging specific to the VmsCoordinator");


double min_latency = 0;

vsg::VmsInterface *vms_interface;

std::vector<simgrid::s4u::CommPtr> pending_comms;

std::unordered_map<std::string, vsg::message> pending_messages;


static void compute_min_latency(){

  min_latency = std::numeric_limits<double>::max();
  
  for(simgrid::s4u::Host *sender : simgrid::s4u::Engine::get_instance()->get_all_hosts()){
    for(simgrid::s4u::Host *receiver : simgrid::s4u::Engine::get_instance()->get_all_hosts()){
      if(sender != receiver){
        std::vector<simgrid::s4u::Link*> links;
        double latency = 0;
        sender->route_to(receiver, links, &latency);
        if(latency < min_latency)
          min_latency = latency;
      }
    }
  }
  
  xbt_assert(min_latency > 0, "error with the platform file : the minimum latency between host is %f  <= 0", min_latency);
  XBT_INFO("the minimum latency on the network is %f sec",min_latency); 
}


static void sender(std::string mailbox_name){

  XBT_INFO("sending a message from host %s",simgrid::s4u::this_actor::get_host()->get_name().c_str());

  int msg_size = pending_messages[mailbox_name].packet_size;
  simgrid::s4u::CommPtr comm = simgrid::s4u::Mailbox::by_name(mailbox_name)->put_async(new std::string(mailbox_name), msg_size);
  pending_comms.push_back(comm);
  comm->wait();
  XBT_INFO("message sent");
}


static void receiver(std::string mailbox_name){
  XBT_INFO("receiving a message from host %s", simgrid::s4u::this_actor::get_host()->get_name().c_str());
  simgrid::s4u::Mailbox::by_name(mailbox_name)->get();
  XBT_INFO("message received");
}

static double get_next_event(){
  double time = simgrid::s4u::Engine::get_clock();
  double next_event_time = std::numeric_limits<double>::max();
  for(simgrid::kernel::resource::Model *model : all_existing_models){
    double model_event = time + model->next_occuring_event(time);
    if(model_event < next_event_time && model_event > time){
      next_event_time = model_event;
    }
  }
  return next_event_time;
}

static void vm_coordinator(){

  while(vms_interface->vmActive()){

    double time = simgrid::s4u::Engine::get_clock();
    double next_reception_time = get_next_event();
    double deadline = std::min(time + min_latency, next_reception_time);

    XBT_DEBUG("simulating to time %f",deadline);

    std::vector<vsg::message> messages = vms_interface->goTo(deadline);

    for(vsg::message m : messages){
      if(m.sent_time > time){
        XBT_DEBUG("sleeping to time %f",m.sent_time);
        simgrid::s4u::this_actor::sleep_until(m.sent_time);
      }	  
      std::string src_host = vms_interface->getHostOfVm(m.src);     
      std::string dest_host = vms_interface->getHostOfVm(m.dest);
      std::string comm_name = m.src + "_" + m.dest + "_" + std::to_string(m.sent_time);
	  
      pending_messages[comm_name] = m;
      XBT_INFO("exchanging data [%s] from vm %s to vm %s", m.data.c_str(), m.src.c_str(), m.dest.c_str());	  
      simgrid::s4u::Actor::create(comm_name + "_sender", simgrid::s4u::Host::by_name(src_host), sender, comm_name);
      simgrid::s4u::Actor::create(comm_name + "_receiver", simgrid::s4u::Host::by_name(dest_host), receiver, comm_name);  
   }

   simgrid::s4u::this_actor::sleep_until(deadline);

   int changed_pos = simgrid::s4u::Comm::test_any(&pending_comms);

   while( changed_pos >= 0 ) { //deadline was on next_reception_time, ie, latency was high enough for the next msg to arrive before this
     simgrid::s4u::CommPtr comm = pending_comms[changed_pos];
     pending_comms.erase(pending_comms.begin() + changed_pos);
     std::string comm_name = comm->get_mailbox()->get_name();

     vsg::message m = pending_messages[comm_name];
     XBT_INFO("delivering data [%s] from vm [%s] to vm [%s]", m.data.c_str(), m.src.c_str(), m.dest.c_str());
     vms_interface->deliverMessage(m);
     pending_messages.erase(comm_name);

     changed_pos = simgrid::s4u::Comm::test_any(&pending_comms);
   }
  }
  
  vms_interface->endSimulation();
  XBT_INFO("end of simulation"); 
}


int main(int argc, char *argv[])
{
  xbt_assert(argc > 1, "Usage: %s platform_file\n", argv[0]);

  simgrid::s4u::Engine e(&argc, argv);

  e.load_platform(argv[1]);

  compute_min_latency();  

  std::unordered_map<std::string, std::string> host_deployments;
  for(int i=2;i<argc;i=i+2){
    host_deployments[argv[i]] = argv[i+1];
  }
  
  vms_interface = new vsg::VmsInterface(host_deployments, false);

  simgrid::s4u::Actor::create("vm_coordinator", e.get_all_hosts()[0], vm_coordinator);

  e.run();

  return 0;
}
