#include "VmsInterface.hpp"
#include "ns3/applications-module.h"
#include "ns3/core-module.h"
#include "ns3/internet-module.h"
#include "ns3/network-module.h"
#include "ns3/point-to-point-layout-module.h"
#include "ns3/point-to-point-module.h"
#include "ns3/traffic-control-helper.h"

#include "tinyxml2.h"

#include <cassert>
#include <limits.h>

// Enable to get some log
// #define LOG_MESSAGES 1

#ifdef LOG_MESSAGES
    #define LOG(msg) std::clog << "[" << ns3::Simulator::Now().ToDouble(ns3::Time::S) << "s] " << msg << std::endl
#else
    #define LOG(msg) do {} while(0)
#endif

#define DEFAULT_SOCKET_NAME "/tmp/ns3_connection_socket"

#define MAX_NODES 100

vsg::VmsInterface *vms_interface;

/* Messages currently in the simulation */
std::vector<ns3::Ptr<ns3::Packet>> pending_packets;
std::vector<vsg::Message *> pending_messages;

/* Messages that that were delivered in the simulation */
std::list<vsg::Message *> ready_to_deliver;

/* ns-3 net devices and addresses for each actor */
std::vector<ns3::Ptr<ns3::PointToPointNetDevice>> tansiv_actors;
std::vector<ns3::Address> tansiv_addresses;
std::vector<ns3::Address> tansiv_mac_addresses;

const std::string vsg_vm_name = "vsg_vm";

double force_min_latency = -1;
double min_latency;

int header_size;

ns3::InternetStackHelper internet;
ns3::PointToPointHelper pointToPoint;
ns3::NodeContainer hub;
ns3::NodeContainer spokes;
ns3::NetDeviceContainer hub_devices;
ns3::NetDeviceContainer spokes_devices;
ns3::Ipv4InterfaceContainer hub_interfaces;
ns3::Ipv4InterfaceContainer spokes_interfaces;
int nSpokes = 0;
ns3::Ipv4AddressHelper hub_address_helper;

static double compute_min_latency() { return min_latency; }

static double get_next_event() { return ns3::Simulator::GetNextEventTime().ToDouble(ns3::Time::S); }

static bool packet_received(ns3::Ptr<ns3::NetDevice> device, ns3::Ptr<const ns3::Packet> packet,
                            short unsigned int protocol, const ns3::Address &from) {
  return true;
}

static void tansiv_actor(ns3::Ptr<ns3::PointToPointNetDevice> ns3_net_device, ns3::Address ip_ns3,
                         ns3::Address mac_address, std::string host_name, std::string ip, std::string file,
                         std::vector<std::string> cmd_line) {
  // IMPORTANT: before any simcall, we register the VM to the interface. This way, the coordinator actor will start
  // AFTER all the registrations.
  LOG("Registering VM " << host_name);
  vms_interface->register_vm(host_name, ip, file, cmd_line);
  // Create ns3 node
  LOG("Registering NetDevice of " << host_name);
  ns3_net_device->SetReceiveCallback(ns3::MakeCallback(&packet_received));
  tansiv_actors.push_back(ns3_net_device);

  LOG("Registering Address " << ip_ns3 << " of " << host_name);
  tansiv_addresses.push_back(ip_ns3);
  tansiv_mac_addresses.push_back(mac_address);
}

static void vm_coordinator() {

  double min_latency = compute_min_latency();

  while (vms_interface->vmActive()) {

    // first we check if a VM stops. If so, we recompute the minimum latency.
    // then we go forward with the VM.
    double time = ns3::Simulator::Now().ToDouble(ns3::Time::S);
    double deadline = time + min_latency;

    LOG("next deadline = " << deadline << " [time+min_latency=" << time + min_latency << "]");

    std::vector<vsg::Message *> messages = vms_interface->goTo(deadline);
    for (vsg::Message *m : messages) {
      time = ns3::Simulator::Now().ToDouble(ns3::Time::S);
      double send_timeeps = m->sent_time + std::numeric_limits<double>::epsilon();
      if (m->sent_time + send_timeeps <= time)
        LOG("violation of the causality constraint : trying to send a message at time " << m->sent_time << "["
                                                                                        << send_timeeps
                                                                                        << "] whereas we are already "
                                                                                           "at time "
                                                                                        << time << "[" << time << "]");

      if (m->sent_time > time) {
        LOG("going to time " << m->sent_time);
        ns3::Simulator::Stop(ns3::Time::FromDouble(m->sent_time, ns3::Time::S) - ns3::Simulator::Now());
        ns3::Simulator::Run();
      }

      std::string src_host_name = vms_interface->getHostOfVm(m->src);
      if (src_host_name.empty()) {
        LOG("Message source is " << m->src);
        LOG("Message dest is " << m->dst);
        LOG("The VM tries to send a message but we do not know its PM");
      }
      // To have usable pcap traces, let's copy the buffer content into the ns-3
      // packet
      // Do not copy:
      // virtio-net header (12 bytes)
      // the ethernet header (14 next bytes)
      // The IP header (20 next bytes)
      const uint8_t *buffer = m->data + header_size + 20;

      ns3::Ptr<ns3::Packet> p = ns3::Create<ns3::Packet>(buffer, m->size - header_size - 20);

      // Useless to copy the other fields?
      ns3::Ipv4Header ipHeader;

      ptrdiff_t pos_src =
          std::find(tansiv_addresses.begin(), tansiv_addresses.end(), ns3::Ipv4Address(m->src.c_str())) -
          tansiv_addresses.begin();
      ptrdiff_t pos_dst =
          std::find(tansiv_addresses.begin(), tansiv_addresses.end(), ns3::Ipv4Address(m->dst.c_str())) -
          tansiv_addresses.begin();
      if (pos_src >= tansiv_addresses.size()) {
        LOG("Source address " << m->src.c_str() << " not found!");
      } else if (pos_dst >= tansiv_addresses.size()) {
        LOG("Destination address " << m->dst.c_str() << " not found!");
      } else {
        ns3::Ipv4Address dest_ipv4_address = ns3::Ipv4Address::ConvertFrom(tansiv_addresses[pos_dst]);
        ns3::Ipv4Address src_ipv4_address = ns3::Ipv4Address::ConvertFrom(tansiv_addresses[pos_src]);
        ipHeader.SetDestination(dest_ipv4_address);
        ipHeader.SetSource(src_ipv4_address);
        ipHeader.SetTtl(m->data[header_size + 8]);      // TTL is the 9th byte of the ip header
        ipHeader.SetProtocol(m->data[header_size + 9]); // Protocol is the 10th byte of the ip header
        ipHeader.SetPayloadSize(p->GetSize());
        p->AddHeader(ipHeader);

        pending_packets.push_back(p);
        pending_messages.push_back(m);

        LOG("Inserting message from " << m->src.c_str() << " to " << m->dst.c_str() << " of size " << m->size);
        tansiv_actors[pos_src]->Send(p, tansiv_addresses[pos_dst], 0x0800);
      }
    }

    // if deadline = infinity, then (1) there is only one remaining VM, and (2) it stops its execution
    // so we do not have to sleep until "infinity" because the simulation is done
    if (deadline != std::numeric_limits<double>::infinity()) {
      ns3::Simulator::Stop(ns3::Time::FromDouble(deadline, ns3::Time::S) - ns3::Simulator::Now());
      ns3::Simulator::Run();
    }

    // Now get all the messages that will be received in the next time slice


    ns3::Time next_deadline =  ns3::Simulator::Now() + ns3::Time::FromDouble(min_latency, ns3::Time::Unit::S);
    // LOG("Getting all events until " << next_deadline << " (Now is " << ns3::Simulator::Now() << " )");

    std::vector<std::tuple<ns3::Time, uint64_t, uint32_t>> next_events = ns3::Simulator::GetNextEventsUntil(next_deadline);

    for (const auto& elem: next_events) {
      ns3::Time receive_date;
      uint64_t packet_id;
      uint32_t dest_id;

      std::tie(receive_date, packet_id, dest_id) = elem;

      // Ignore hub event
      if (dest_id == 0) {
        continue;
      }

      // Find the message
      auto it = std::find_if(pending_packets.begin(), pending_packets.end(),
                         [&packet_id](ns3::Ptr<ns3::Packet> &arg) { return arg->GetUid() == packet_id; });
      if (it == pending_packets.end()) {
        //Packet not in pending_packets
        LOG("Received packet is not in pending_packets!");
      } else {
        int index = std::distance(pending_packets.begin(), it);
        vsg::Message *message = pending_messages[index];
        message->receive_date = (uint64_t) receive_date.ToInteger(ns3::Time::NS);
        ready_to_deliver.push_back(message);
        pending_packets.erase(pending_packets.begin() + index);
        pending_messages.erase(pending_messages.begin() + index);
      }

    }

    while (ready_to_deliver.size() > 0) {
      vsg::Message *m = ready_to_deliver.front();
      ready_to_deliver.pop_front();

      LOG("[coordinator]: delivering data from vm [" << m->src << "] to vm [" << m->dst << "] (size=" << m->size
                                                     << " receive_date=" << m->receive_date << " )");
      vms_interface->deliverMessage(m);
    }
    LOG("Timestep finished preparing the next iteration [current_time="
        << ns3::Simulator::Now().ToDouble(ns3::Time::S) << "]");
  }
  vms_interface->end_simulation(true, false);
  LOG("end of simulation");
}

int lookup_args(std::string argname, int argc, char *argv[]) {
  for (int i = 1; i < argc; ++i) {
    if (std::string(argv[i]) == argname) {
      return i + 1;
    }
  }
  return -1;
}

double lookup_args_double(std::string argname, double default_value, int argc, char *argv[]) {
  std::string outvalue;
  int idx = lookup_args(argname, argc, argv);
  // handle errors
  if (idx == -1) {
    return default_value;
  }
  return std::stod(argv[idx]);
}

std::string lookup_args_str(std::string argname, std::string default_value, int argc, char *argv[]) {
  int idx = lookup_args(argname, argc, argv);
  // handle errors
  if (idx == -1) {
    return default_value;
  }
  return std::string(argv[idx]);
}

void create_star(std::string latency, std::string bandwidth) {
  ns3::Time::SetResolution(ns3::Time::NS);
  ns3::Config::SetDefault("ns3::RateErrorModel::ErrorRate", ns3::DoubleValue(0));
  ns3::Config::SetDefault("ns3::BurstErrorModel::ErrorRate", ns3::DoubleValue(0));

  LOG("Set default queue size");
  // Global
  ns3::Config::SetDefault(
      "ns3::DropTailQueue<Packet>::MaxSize",
      ns3::QueueSizeValue(ns3::QueueSize(ns3::QueueSizeUnit::PACKETS, 100))); // ns-3 supports either bytes or packets
  pointToPoint.SetDeviceAttribute("DataRate", ns3::StringValue(bandwidth));
  // Use MTU > 1500 to avoid having packets splitted by ns-3 (Possible with the
  // added PPP header)
  // We assume the default 1500 MTU is enforced somewhere else
  pointToPoint.SetDeviceAttribute("Mtu", ns3::UintegerValue(3000));
  LOG("Setting latency " << latency);
  pointToPoint.SetChannelAttribute("Delay", ns3::StringValue(latency));
  pointToPoint.DisableFlowControl();

  hub.Create(1);
  spokes.Create(MAX_NODES);
  internet.Install(hub);
  internet.Install(spokes);
}

void add_star_spoke(std::string host_name, std::string ip, std::string mask, ns3::Time ifg, std::string mac,
                    std::string boot_command, std::vector<std::string> boot_args) {
  // Create P2P link with the hub
  LOG("Creating P2P link between node and hub");
  ns3::NetDeviceContainer nd = pointToPoint.Install(hub.Get(0), spokes.Get(nSpokes));
  hub_devices.Add(nd.Get(0));
  spokes_devices.Add(nd.Get(1));

  // Assign address to the node
  LOG("Assigning IP addresses");

  ns3::Ptr<ns3::Ipv4> ipv4 = spokes.Get(nSpokes)->GetObject<ns3::Ipv4>();
  int32_t interface = ipv4->AddInterface(spokes_devices.Get(nSpokes));
  ns3::Ipv4InterfaceAddress address =
      ns3::Ipv4InterfaceAddress(ns3::Ipv4Address(ip.c_str()), ns3::Ipv4Mask(mask.c_str()));

  ipv4->AddAddress(interface, address);

  ipv4->SetUp(interface);

  ns3::Ipv4InterfaceContainer spoke_ipv4_container;

  spoke_ipv4_container.Add(ipv4, interface);

  spokes_interfaces.Add(spoke_ipv4_container);

  // Assign address to the hub
  hub_address_helper.Assign(hub_devices.Get(nSpokes));

  // Get net_devices
  LOG("Getting net devices");
  ns3::Ptr<ns3::PointToPointNetDevice> net_device =
      ns3::StaticCast<ns3::PointToPointNetDevice>(spokes_devices.Get(nSpokes));
  ns3::Ptr<ns3::PointToPointNetDevice> net_device_hub =
      ns3::StaticCast<ns3::PointToPointNetDevice>(hub_devices.Get(nSpokes));

  // Set MAC address
  LOG("Setting MAC addresses");
  ns3::Address mac_address = ns3::Mac48Address(mac.c_str());
  net_device->SetAddress(mac_address);

  // Set Inter Frame Gap
  LOG("Setting IFG");
  net_device->SetInterframeGap(ifg);
  net_device_hub->SetInterframeGap(ifg);

  // Get address
  LOG("Getting IP address");
  ns3::Address ip_ns3 = spokes_interfaces.GetAddress(nSpokes);

  // Set infinite queue size on node
  LOG("Setting infinite queue size on node");
  ns3::QueueSizeValue queue_size = ns3::QueueSize(ns3::QueueSizeUnit::PACKETS, UINT32_MAX);
  ns3::PointerValue ptr;
  net_device->GetAttribute("TxQueue", ptr);
  ns3::Ptr<ns3::Queue<ns3::Packet>> txQueue = ptr.Get<ns3::Queue<ns3::Packet>>();
  ns3::Ptr<ns3::DropTailQueue<ns3::Packet>> dtq = txQueue->GetObject<ns3::DropTailQueue<ns3::Packet>>();
  dtq->SetAttribute("MaxSize", queue_size);

  nSpokes++;

  LOG("Converting IP to string");
  ns3::Ipv4Address ip_ns3_ipv4 = ns3::Ipv4Address::ConvertFrom(ip_ns3);
  uint8_t buf[4];
  ip_ns3_ipv4.Serialize(buf);
  std::string ip_str = std::to_string(buf[0]) + "." + std::to_string(buf[1]) + "." + std::to_string(buf[2]) + "." +
                       std::to_string(buf[3]);
  LOG("IP string is " << ip_str);

  // Create TANSIV actor
  tansiv_actor(net_device, ip_ns3, mac_address, host_name, ip_str, boot_command, boot_args);
}

std::string parse_actor_field(tinyxml2::XMLElement *actor, const char *field) {
  tinyxml2::XMLElement *element = actor->FirstChildElement(field);
  std::string element_text = element->Attribute("value");
  LOG(field << " is " << element_text);
  return element_text;
}

double bandwidth_str_to_double(std::string bandwidth) {
  if (bandwidth.find("Gbps") != std::string::npos) {
    return std::stod(bandwidth.substr(0, bandwidth.size() - 4)) * 1e9;
  }
  if (bandwidth.find("Mbps") != std::string::npos) {
    return std::stod(bandwidth.substr(0, bandwidth.size() - 4)) * 1e6;
  }
  if (bandwidth.find("Kbps") != std::string::npos) {
    return std::stod(bandwidth.substr(0, bandwidth.size() - 4)) * 1e3;
  }
  if (bandwidth.find("bps") != std::string::npos) {
    return std::stod(bandwidth.substr(0, bandwidth.size() - 3));
  }
  throw std::invalid_argument("Invalid bandwidth format.");
}

std::string bandwdith_str_to_bps(std::string bandwidth) {
  if (bandwidth.find("Gbps") != std::string::npos) {
    return std::to_string(std::stoull(bandwidth.substr(0, bandwidth.size() - 4)) * 1'000'000'000);
  }
  if (bandwidth.find("Mbps") != std::string::npos) {
    return std::to_string(std::stoull(bandwidth.substr(0, bandwidth.size() - 4)) * 1'000'000);
  }
  if (bandwidth.find("Kbps") != std::string::npos) {
    return std::to_string(std::stoull(bandwidth.substr(0, bandwidth.size() - 4)) * 1'000);
  }
  if (bandwidth.find("bps") != std::string::npos) {
    return std::to_string(std::stoull(bandwidth.substr(0, bandwidth.size() - 3)) * 1);
  }
  throw std::invalid_argument("Invalid bandwidth format.");
}

int main(int argc, char *argv[]) {
  // Parse coordinator args
  std::string socket_name = lookup_args_str("--socket_name", DEFAULT_SOCKET_NAME, argc, argv);

  force_min_latency = lookup_args_double("--force", -1, argc, argv);

  LOG("Forcing the minimum latency to " << force_min_latency);

  vms_interface = new vsg::VmsInterface(socket_name);

  // Set up dummy IP addresses for the hub interfaces
  hub_address_helper.SetBase("0.0.0.0", "255.255.255.0");

  // Parse platform file
  tinyxml2::XMLDocument platform;
  auto err = platform.LoadFile(argv[1]);
  if (err != tinyxml2::XML_SUCCESS) {
    LOG("Error: failed to platform deployment file!");
    return 1;
  }
  tinyxml2::XMLElement *platform_elem = platform.FirstChildElement("platform");
  std::string bandwidth = parse_actor_field(platform_elem, "bandwidth");
  std::string latency = parse_actor_field(platform_elem, "latency");
  min_latency = std::stod(parse_actor_field(platform_elem, "min_latency"));
  header_size = std::stoi(parse_actor_field(platform_elem, "header_size"));

  create_star(latency, bandwidth);

  // Parse deployment file
  tinyxml2::XMLDocument deployment;
  err = deployment.LoadFile(argv[2]);
  if (err != tinyxml2::XML_SUCCESS) {
    LOG("Error: failed to load deployment file!");
    return 1;
  }
  // Iterate over all actors
  tinyxml2::XMLElement *deployment_platform_elem = deployment.FirstChildElement("platform");
  tinyxml2::XMLElement *actor = deployment_platform_elem->FirstChildElement("actor");
  LOG("Starting to parse deployment file");
  while (actor != nullptr) {
    std::string host_name = actor->Attribute("host");
    LOG("Host name is " << host_name);

    std::string ip = parse_actor_field(actor, "ip");

    std::string mask = parse_actor_field(actor, "mask");

    double ifg = std::stod(parse_actor_field(actor, "ifg"));

    std::string mac = parse_actor_field(actor, "mac");

    std::string boot_script = parse_actor_field(actor, "boot_script");

    std::vector<std::string> boot_args;
    boot_args.push_back(boot_script);
    tinyxml2::XMLElement *argument = actor->FirstChildElement("argument");
    while (argument != nullptr) {
      std::string argument_text = argument->Attribute("value");
      boot_args.push_back(argument_text);
      LOG("Argument value is " << argument_text);
      argument = argument->NextSiblingElement("argument");
    }
    // Push bandwidth to boot args
    boot_args.push_back("--vsg_bandwidth");
    boot_args.push_back(bandwdith_str_to_bps(bandwidth));
    // We can use the default value (24) for the ethernet overhead

    ns3::Time ifg_time = ns3::Time::FromDouble((ifg * 8) / bandwidth_str_to_double(bandwidth), ns3::Time::S);
    LOG("ifg_time is " << ifg_time);
    add_star_spoke(host_name, ip, mask, ifg_time, mac, boot_script, boot_args);

    actor = actor->NextSiblingElement("actor");
  }

  // Populate routing tables
  ns3::Ipv4GlobalRoutingHelper::PopulateRoutingTables();

  vm_coordinator();

  delete vms_interface;

  return 0;
}
