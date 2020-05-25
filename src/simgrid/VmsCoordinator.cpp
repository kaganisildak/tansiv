#include "VmsInterface.hpp"
#include "simgrid/s4u.hpp"
#include <limits.h>
#include <simgrid/kernel/resource/Model.hpp>

XBT_LOG_NEW_DEFAULT_CATEGORY(vm_coordinator, "Logging specific to the VmsCoordinator");

vsg::VmsInterface* vms_interface;

std::vector<simgrid::s4u::CommPtr> pending_comms;

std::vector<vsg::message> pending_messages;

static std::vector<simgrid::s4u::ActorPtr> receivers;

const std::string vsg_vm_name = "vsg_vm";

double force_min_latency = -1;

static double compute_min_latency()
{
  if (force_min_latency >= 0) {
    return force_min_latency;
  }
  double min_latency = std::numeric_limits<double>::infinity();

  for (simgrid::s4u::ActorPtr sender : receivers) {
    for (simgrid::s4u::ActorPtr receiver : receivers) {
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
  double time            = simgrid::s4u::Engine::get_clock();
  double next_event_time = std::numeric_limits<double>::infinity();
  for (simgrid::kernel::resource::Model* model : all_existing_models) {
    double model_event = time + model->next_occuring_event(time);
    if (model_event < next_event_time && model_event > time) {
      next_event_time = model_event;
    }
  }
  return next_event_time;
}

static void sender(std::string mailbox_name, vsg::message m)
{

  XBT_INFO("sending [%s] (size %lu) from vm [%s], to vm [%s] (on pm [%s])", m.data.c_str(), m.packet_size,
           m.src.c_str(), m.dest.c_str(), mailbox_name.c_str());

  int msg_size               = m.packet_size;
  simgrid::s4u::CommPtr comm = simgrid::s4u::Mailbox::by_name(mailbox_name)->put_async(&m, msg_size);
  pending_comms.push_back(comm);
  pending_messages.push_back(m);
  comm->wait();
  // for the receiver to be able to get the message (just useful for login purpose)
  simgrid::s4u::this_actor::yield();
}

static void receiver(std::vector<std::string> args)
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
  receivers.push_back(simgrid::s4u::Actor::self());

  simgrid::s4u::ActorPtr myself  = simgrid::s4u::Actor::self();
  simgrid::s4u::Mailbox* mailbox = simgrid::s4u::Mailbox::by_name(mailbox_name);

  // this actor is a permanent receiver on its host mailbox.
  mailbox->set_receiver(myself);
  // For the simulation to end with the coordinator actor, we daemonize all the other actors.
  myself->daemonize();

  while (true) {
    vsg::message* m = static_cast<vsg::message*>(mailbox->get());
    XBT_INFO("[receiver] delivering data [%s] from vm [%s] to vm [%s]", m->data.c_str(), m->src.c_str(),
             m->dest.c_str());
  }
}

static void vm_coordinator()
{

  // IMPORTANT: we ensure that all the receiver actors registered their VMs to the interface before going further
  simgrid::s4u::this_actor::yield();
  double min_latency = compute_min_latency();

  while (vms_interface->vmActive()) {

    // first we check if a VM stops. If so, we recompute the minimum latency.
    bool deads = false;
    for (auto host : vms_interface->get_dead_vm_hosts()) {

      auto erased_section_begin =
          std::remove_if(receivers.begin(), receivers.end(), [host](const simgrid::s4u::ActorPtr& o) {
            if (o->get_host()->get_name() == host) {
              return true;
            }
            return false;
          });

      receivers.erase(erased_section_begin, receivers.end());
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

    std::vector<vsg::message> messages = vms_interface->goTo(deadline);

    for (vsg::message m : messages) {

      time                = simgrid::s4u::Engine::get_clock();
      double send_timeeps = m.sent_time + std::numeric_limits<double>::epsilon();
      xbt_assert(
          m.sent_time + send_timeeps >= time,
          "violation of the causality constraint : trying to send a message at time %f[%f] whereas we are already "
          "at time %f[%f]",
          m.sent_time, send_timeeps, time, time);
      if (m.sent_time > time) {
        XBT_DEBUG("going to time %f", m.sent_time);
        simgrid::s4u::this_actor::sleep_until(m.sent_time);
      }

      std::string src_host = vms_interface->getHostOfVm(m.src);
      xbt_assert(src_host != "", "The VM %s tries to send a message but we do not know its PM", m.src.c_str());

      std::string dest_host = vms_interface->getHostOfVm(m.dest);
      if (dest_host != "") {
        simgrid::s4u::ActorPtr actor =
            simgrid::s4u::Actor::create("sender", simgrid::s4u::Host::by_name(src_host), sender, dest_host, m);
        // For the simulation to end with the coordinator actor, we daemonize all the other actors.
        actor->daemonize();
      } else {
        XBT_WARN("the VM %s tries to send a message to the unknown VM %s", m.src.c_str(), m.dest.c_str());
      }
    }

    // if deadline = infinity, then (1) there is only one remaining VM, and (2) it stops its execution
    // so we do not have to sleep until "infinity" because the simulation is done
    if (deadline != std::numeric_limits<double>::infinity()) {
      double time = simgrid::s4u::Engine::get_clock();
      simgrid::s4u::this_actor::sleep_until(deadline);
    }

    int changed_pos = simgrid::s4u::Comm::test_any(&pending_comms);

    while (
        changed_pos >=
        0) { // deadline was on next_reception_time, ie, latency was high enough for the next msg to arrive before this
      simgrid::s4u::CommPtr comm = pending_comms[changed_pos];
      vsg::message m             = pending_messages[changed_pos];

      pending_comms.erase(pending_comms.begin() + changed_pos);
      pending_messages.erase(pending_messages.begin() + changed_pos);

      XBT_DEBUG("[coordinator]: delivering data [%s] from vm [%s] to vm [%s]", m.data.c_str(), m.src.c_str(),
                m.dest.c_str());
      vms_interface->deliverMessage(m);

      changed_pos = simgrid::s4u::Comm::test_any(&pending_comms);
    }
    XBT_DEBUG("Timestep finished preparing the next iteration [current_time=%f] [next_event = %f]",
              simgrid::s4u::Engine::get_clock(), get_next_event());
  }

  vms_interface->end_simulation(true, false);
  XBT_INFO("end of simulation");
}

double parse_args_force(int argc, char* argv[])
{
  for (int i = 1; i < argc; ++i) {
    if (std::string(argv[i]) == "--force") {
      return std::stod(argv[i + 1]);
    }
  }
  return -1;
}

int main(int argc, char* argv[])
{
  xbt_assert(argc > 2, "Usage: %s platform_file deployment_file\n", argv[0]);

  force_min_latency = parse_args_force(argc, argv);
  XBT_DEBUG("Forcing the minimum latency to %d", force_min_latency);

  simgrid::s4u::Engine e(&argc, argv);

  e.load_platform(argv[1]);

  vms_interface = new vsg::VmsInterface();

  e.register_function(vsg_vm_name, &receiver);

  simgrid::s4u::Actor::create("vm_coordinator", e.get_all_hosts()[0], vm_coordinator);

  e.load_deployment(argv[2]);

  e.run();

  return 0;
}
