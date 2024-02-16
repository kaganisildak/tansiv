#include "VmsInterface.hpp"
#include "simgrid/s4u.hpp"
#include <limits.h>

// Default socket name
// - can be overriden using the --socket_name flag of the commandline
// - the socket name is pushed to the client as their first argument
#define DEFAULT_SOCKET_NAME "/tmp/simgrid_connection_socket"

XBT_LOG_NEW_DEFAULT_CATEGORY(vm_coordinator, "Logging specific to the VmsCoordinator");

vsg::VmsInterface* vms_interface;

std::vector<simgrid::s4u::CommPtr> pending_comms;

std::vector<vsg::Message*> pending_messages;

static std::vector<simgrid::s4u::ActorPtr> tansiv_actors;

const std::string vsg_vm_name = "vsg_vm";

double force_min_latency = -1;

static double compute_min_latency()
{
  if (force_min_latency >= 0) {
    return force_min_latency;
  }
  double min_latency = std::numeric_limits<double>::infinity();

  for (simgrid::s4u::ActorPtr const& sender : tansiv_actors) {
    for (simgrid::s4u::ActorPtr const& receiver : tansiv_actors) {
      if (sender != receiver) {
        std::vector<simgrid::s4u::Link*> links;
        double latency = 0;
        sender->get_host()->route_to(receiver->get_host(), links, &latency);
        if (latency < min_latency)
          min_latency = latency;
      }
    }
  }

  xbt_assert(min_latency > 0, "error with the platform file : the minimum latency between hosts is %f  <= 0",
             min_latency);
  XBT_INFO("the minimum latency on the network is %f sec", min_latency);
  return min_latency;
}

static double get_next_event()
{
  simgrid::s4u::Engine* engine = simgrid::s4u::Engine::get_instance();
  double time            = simgrid::s4u::Engine::get_clock();
  double next_event_time = std::numeric_limits<double>::infinity();
  for (auto model : engine->get_all_models()) {
    double model_event = time + model->next_occurring_event(time);
    if (model_event < next_event_time && model_event > time) {
      next_event_time = model_event;
    }
  }
  return next_event_time;
}

static void tansiv_actor(std::vector<std::string> args)
{

  XBT_INFO("running receiver");

  // we consider only one mailbox per host. The mailbox has the name of its host.
  std::string mailbox_name = simgrid::s4u::this_actor::get_host()->get_name();
  // we separate the first two arguments, that correspond respectively the VM ID and the executable name, to the other
  // ones, that correspond to the arguments used to launch the VM executable.
  xbt_assert(args.size() >= 3,
             "need at least two arguments to launch a %s: (1) the VM ID, and (2) the executable name. You should fix "
             "your deployment file.",
             vsg_vm_name.c_str());
  std::vector<std::string> fork_command(args.size() - 2);
  std::copy(args.begin() + 2, args.end(), fork_command.begin());

  // IMPORTANT: before any simcall, we register the VM to the interface. This way, the coordinator actor will start
  // AFTER all the registrations.
  vms_interface->register_vm(mailbox_name, args[1], args[2], fork_command);
  tansiv_actors.push_back(simgrid::s4u::Actor::self());
}

static void vm_coordinator()
{

  // IMPORTANT: we ensure that all the receiver actors registered their VMs to the interface before going further
  simgrid::s4u::this_actor::yield();
  double min_latency = compute_min_latency();

  while (vms_interface->vmActive()) {

    // first we check if a VM stops. If so, we recompute the minimum latency.
    bool deads = false;
    for (auto const& host : vms_interface->get_dead_vm_hosts()) {

      auto erased_section_begin =
          std::remove_if(tansiv_actors.begin(), tansiv_actors.end(),
                         [host](const simgrid::s4u::ActorPtr& o) { return (o->get_host()->get_name() == host); });

      tansiv_actors.erase(erased_section_begin, tansiv_actors.end());
      deads = true;
    }
    if (deads)
      min_latency = compute_min_latency();

    // then we go forward with the VM.
    double time                = simgrid::s4u::Engine::get_clock();
    double next_reception_time = get_next_event();
    double deadline            = std::min(time + min_latency, next_reception_time);

    XBT_DEBUG("next deadline = %f [time+min_latency=%f, next_reception_time=%f]", deadline, time + min_latency,
              next_reception_time);

    std::vector<vsg::Message*> messages = vms_interface->goTo(deadline);
    for (vsg::Message* m : messages) {
      time                = simgrid::s4u::Engine::get_clock();
      double send_timeeps = m->sent_time + std::numeric_limits<double>::epsilon();
      xbt_assert(
          m->sent_time + send_timeeps >= time,
          "violation of the causality constraint : trying to send a message at time %f[%f] whereas we are already "
          "at time %f[%f]",
          m->sent_time, send_timeeps, time, time);
      if (m->sent_time > time) {
        XBT_DEBUG("going to time %f", m->sent_time);
        simgrid::s4u::this_actor::sleep_until(m->sent_time);
      }


      std::string src_host_name = vms_interface->getHostOfVm(m->src);
      xbt_assert(not src_host_name.empty(), "The VM %s tries to send a message but we do not know its PM",
                 m->src.c_str());

      std::string dest_host_name = vms_interface->getHostOfVm(m->dst);
      if (not dest_host_name.empty()) {
        auto src_host  = simgrid::s4u::Host::by_name(src_host_name);
        auto dest_host = simgrid::s4u::Host::by_name(dest_host_name);
	// Ethernet adds an overhead of 24 bytes per packet: preamble + frame start delimiter =  8
	//                                                   frame checksum (FCS)             =  4
	//                                                   inter packet gap (IGP)           = 12
        auto comm      = simgrid::s4u::Comm::sendto_async(src_host, dest_host, m->size + 24);
        pending_comms.push_back(comm);
        pending_messages.push_back(m);
      } else {
        XBT_WARN("the VM %s tries to send a message to the unknown VM %s", m->src.c_str(), m->dst.c_str());
      }
    }

    // if deadline = infinity, then (1) there is only one remaining VM, and (2) it stops its execution
    // so we do not have to sleep until "infinity" because the simulation is done
    if (deadline != std::numeric_limits<double>::infinity()) {
      simgrid::s4u::this_actor::sleep_until(deadline);
    }
    int changed_pos = simgrid::s4u::Comm::test_any(pending_comms);
    while (
        changed_pos >=
        0) { // deadline was on next_reception_time, ie, latency was high enough for the next msg to arrive before this
      simgrid::s4u::CommPtr comm = pending_comms[changed_pos];
      vsg::Message* m            = pending_messages[changed_pos];

      pending_comms.erase(pending_comms.begin() + changed_pos);
      pending_messages.erase(pending_messages.begin() + changed_pos);

      XBT_INFO("[coordinator]: delivering data from vm [%s] to vm [%s] (size=%d, sent_time=%f)", m->src.c_str(), m->dst.c_str(),
               m->size, m->sent_time);
      vms_interface->deliverMessage(m);

      changed_pos = simgrid::s4u::Comm::test_any(pending_comms);
    }
    XBT_DEBUG("Timestep finished preparing the next iteration [current_time=%f] [next_event = %f]",
              simgrid::s4u::Engine::get_clock(), get_next_event());
  }

  vms_interface->end_simulation(true, false);
  XBT_INFO("end of simulation");
}

int lookup_args(std::string argname, int argc, char* argv[])
{
  for (int i = 1; i < argc; ++i) {
    if (std::string(argv[i]) == argname) {
      return i + 1;
    }
  }
  return -1;
}

double lookup_args_double(std::string argname, double default_value, int argc, char* argv[])
{
  std::string outvalue;
  int idx = lookup_args(argname, argc, argv);
  // handle errors
  if (idx == -1) {
    return default_value;
  }
  return std::stod(argv[idx]);
}

std::string lookup_args_str(std::string argname, std::string default_value, int argc, char* argv[])
{
  int idx = lookup_args(argname, argc, argv);
  // handle errors
  if (idx == -1) {
    return default_value;
  }
  return std::string(argv[idx]);
}

int main(int argc, char* argv[])
{
  xbt_assert(argc > 2, "Usage: %s platform_file deployment_file\n", argv[0]);

  force_min_latency       = lookup_args_double("--force", -1, argc, argv);
  std::string socket_name = lookup_args_str("--socket_name", DEFAULT_SOCKET_NAME, argc, argv);

  simgrid::s4u::Engine e(&argc, argv);

  XBT_DEBUG("Forcing the minimum latency to %f\n", force_min_latency);
  XBT_DEBUG("Using %s as socket name\n", socket_name.c_str());

  e.load_platform(argv[1]);
  // Mark the upload links in the cluster as serial. This won't work when we use something else than clusters.
  for (auto l : e.get_all_links()) {
     auto name = l->get_name();
     const char* pattern = "_UP"; // UP links are automatically created for <cluster> tags
     if (name.compare (name.length() - strlen(pattern), strlen(pattern), std::string(pattern)) == 0) {
       XBT_INFO("Setting link '%s' as serial", name.c_str());
       l->set_concurrency_limit(1);
     }
  }

  vms_interface = new vsg::VmsInterface(socket_name);

  e.register_function(vsg_vm_name, &tansiv_actor);

  simgrid::s4u::Actor::create("vm_coordinator", e.get_all_hosts()[0], vm_coordinator);

  e.load_deployment(argv[2]);

  e.run();

  delete vms_interface;

  return 0;
}
