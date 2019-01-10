#include "simgrid/s4u.hpp"
#include "VmsInterface.hpp"
#include "vsg.h"
#include <simgrid/kernel/resource/Model.hpp>
#include <limits.h>

XBT_LOG_NEW_DEFAULT_CATEGORY(vm_coordinator, "Logging specific to the VmsCoordinator");


double min_latency = 0;

int nb_comm = 0;

vsg::VmsInterface *vms_interface;

std::vector<simgrid::s4u::CommPtr> pending_comms;

std::vector<vsg::message> pending_messages;

std::vector<simgrid::s4u::Host*> hosts;

static void compute_min_latency(){

  min_latency = std::numeric_limits<double>::max();
  
  for(simgrid::s4u::Host *sender : hosts){
    for(simgrid::s4u::Host *receiver : hosts){
      if(sender != receiver){
        std::vector<simgrid::s4u::Link*> links;
        double latency = 0;
        sender->route_to(receiver, links, &latency);
        if(latency < min_latency)
          min_latency = latency;
      }
    }
  }
  
  xbt_assert(min_latency > 0, "error with the platform file : the minimum latency between hosts is %f  <= 0", min_latency);
  XBT_INFO("the minimum latency on the network is %f sec",min_latency); 
}

static void sender(std::string mailbox_name, vsg::message m){

  XBT_INFO("sending [%s] (size %lu) from vm [%s], to vm [%s] (on pm [%s])", m.data.c_str(), m.packet_size, m.src.c_str(), m.dest.c_str(), mailbox_name.c_str());

  int msg_size = m.packet_size;
  simgrid::s4u::CommPtr comm = simgrid::s4u::Mailbox::by_name(mailbox_name)->put_async(&m, msg_size);
  pending_comms.push_back(comm);
  pending_messages.push_back(m);
  comm->wait();
}

static void receiver()
{
  std::string mailbox_name = simgrid::s4u::this_actor::get_host()->get_name();
  simgrid::s4u::MailboxPtr mailbox = simgrid::s4u::Mailbox::by_name(mailbox_name);

  while(true){
    vsg::message *m = static_cast<vsg::message*>(mailbox->get());
    XBT_INFO("delivering data [%s] from vm [%s] to vm [%s]", m->data.c_str(), m->src.c_str(), m->dest.c_str());
  }
}

static void deploy_permanent_receivers(){
  int nb_receiver = 0;
  for(simgrid::s4u::Host *host : hosts){
    simgrid::s4u::ActorPtr actor = simgrid::s4u::Actor::create("receiver_"+std::to_string(nb_receiver), host, receiver);
    nb_receiver++;
    simgrid::s4u::Mailbox::by_name(host->get_name())->set_receiver(actor);
    actor->daemonize();
  }
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

      time =  simgrid::s4u::Engine::get_clock();
      xbt_assert(m.sent_time >= time, "violation of the causality constraint : trying to send a message at time %f whereas we are already at time %f", m.sent_time, time);
      if(m.sent_time > time){
        XBT_DEBUG("going to time %f",m.sent_time);
        simgrid::s4u::this_actor::sleep_until(m.sent_time);
        time = simgrid::s4u::Engine::get_clock();
      }
	  
      std::string src_host = vms_interface->getHostOfVm(m.src);
      std::string dest_host = vms_interface->getHostOfVm(m.dest);     
      std::string comm_name = "sender_"+std::to_string(nb_comm);
      nb_comm++;
      simgrid::s4u::ActorPtr actor = simgrid::s4u::Actor::create(comm_name, simgrid::s4u::Host::by_name(src_host), sender, dest_host, m);
      actor->daemonize();
    }

    simgrid::s4u::this_actor::sleep_until(deadline);

    int changed_pos = simgrid::s4u::Comm::test_any(&pending_comms);

    while( changed_pos >= 0 ) { //deadline was on next_reception_time, ie, latency was high enough for the next msg to arrive before this
      simgrid::s4u::CommPtr comm = pending_comms[changed_pos];
      vsg::message m = pending_messages[changed_pos];

      pending_comms.erase(pending_comms.begin() + changed_pos);
      pending_messages.erase(pending_messages.begin() + changed_pos);

      XBT_DEBUG("delivering data [%s] from vm [%s] to vm [%s]", m.data.c_str(), m.src.c_str(), m.dest.c_str());
      vms_interface->deliverMessage(m);

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

  std::unordered_map<std::string, std::string> host_deployments;

  for(int i=2;i<argc;i=i+2){
    host_deployments[argv[i]] = argv[i+1];
    hosts.push_back(e.host_by_name(argv[i+1]));
  }

  compute_min_latency();
   
  vms_interface = new vsg::VmsInterface(host_deployments, false);

  deploy_permanent_receivers();

  simgrid::s4u::Actor::create("vm_coordinator", e.get_all_hosts()[0], vm_coordinator);

  e.run();

  return 0;
}
