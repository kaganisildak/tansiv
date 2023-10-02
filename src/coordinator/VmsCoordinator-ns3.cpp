#include "VmsInterface.hpp"
#include "ns3/applications-module.h"
#include "ns3/core-module.h"
#include "ns3/internet-module.h"
// #include "ns3/netanim-module.h"
#include "ns3/network-module.h"
#include "ns3/point-to-point-layout-module.h"
#include "ns3/point-to-point-module.h"
#include "ns3/traffic-control-helper.h"

#include <cassert>
#include <limits.h>

#define LOG(msg) std::clog << "[" << ns3::Simulator::Now().ToDouble(ns3::Time::S) << "s] " << msg << std::endl

#define DEFAULT_SOCKET_NAME "/tmp/ns3_connection_socket"

vsg::VmsInterface* vms_interface;

/* Messages currently in the simulation */
std::vector<ns3::Ptr<ns3::Packet>> pending_packets;
std::vector<vsg::Message*> pending_messages;

/* Messages that that were delivered in the simulation */
std::vector<vsg::Message*> ready_to_deliver;

/* ns-3 net devices and addresses for each actor */
std::vector<ns3::Ptr<ns3::PointToPointNetDevice>> tansiv_actors;
std::vector<ns3::Address> tansiv_addresses;
std::vector<ns3::Address> tansiv_mac_addresses;

const std::string vsg_vm_name = "vsg_vm";

double force_min_latency = -1;

std::string latency   = "1ms";
std::string bandwidth = "100Mbps";

static double compute_min_latency()
{
  return 0.001;
}

static double get_next_event()
{
  return ns3::Simulator::GetNextEventTime().ToDouble(ns3::Time::S);
}

static bool packet_received(ns3::Ptr<ns3::NetDevice> device, ns3::Ptr<const ns3::Packet> packet,
                            short unsigned int protocol, const ns3::Address& from)
{
  // LOG("Packet received!");
  int packet_id = packet->GetUid();
  auto it       = std::find_if(pending_packets.begin(), pending_packets.end(),
                               [&packet_id](ns3::Ptr<ns3::Packet>& arg) { return arg->GetUid() == packet_id; });
  if (it == pending_packets.end()) {
    // Packet not in pending_packets
    LOG("Received packet is not in pending_packets!");
  } else {
    int index             = std::distance(pending_packets.begin(), it);
    vsg::Message* message = pending_messages[index];
    ready_to_deliver.push_back(message);
    pending_packets.erase(pending_packets.begin() + index);
    pending_messages.erase(pending_messages.begin() + index);
  }

  return true;
}

static void tansiv_actor(ns3::Ptr<ns3::PointToPointNetDevice> ns3_net_device, ns3::Address ip_ns3,
                         ns3::Address mac_address, std::string host_name, std::string ip, std::string file,
                         std::vector<std::string> cmd_line)
{
  // IMPORTANT: before any simcall, we register the VM to the interface. This way, the coordinator actor will start
  // AFTER all the registrations.
  LOG("Registering VM " << host_name);
  // std::vector<std::string> fork_command(args.size() - 2);
  // std::copy(args.begin() + 2, args.end(), fork_command.begin());
  vms_interface->register_vm(host_name, ip, file, cmd_line);
  // Create ns3 node
  LOG("Registering NetDevice of " << host_name);
  ns3_net_device->SetReceiveCallback(ns3::MakeCallback(&packet_received));
  tansiv_actors.push_back(ns3_net_device);

  LOG("Registering Address " << ip_ns3 << " of " << host_name);
  tansiv_addresses.push_back(ip_ns3);
  tansiv_mac_addresses.push_back(mac_address);
}

static void vm_coordinator()
{

  double min_latency = compute_min_latency();

  while (vms_interface->vmActive()) {

    // first we check if a VM stops. If so, we recompute the minimum latency.
    // TODOs

    // then we go forward with the VM.
    double time                = ns3::Simulator::Now().ToDouble(ns3::Time::S);
    double next_reception_time = get_next_event();
    double deadline            = std::min(time + min_latency - 1e-9, next_reception_time);

    LOG("next deadline = " << deadline << " [time+min_latency=" << time + min_latency - 1e-9
                           << ", next_reception_time=" << next_reception_time << "]");

    std::vector<vsg::Message*> messages = vms_interface->goTo(deadline);
    for (vsg::Message* m : messages) {
      time                = ns3::Simulator::Now().ToDouble(ns3::Time::S);
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
      // Size Adjustment (in bytes)
      // IPG is already set during initialization
      // + 8 (Preamble)
      // + 4 (CRC)
      // -2 (ns-3 PPP header)
      // -20 (ns-3 Ipv4 header)
      // -12 (virtio-net header)
      // = -22

      // assert(m->size >= 22);

      // To have usable pcap traces, let's copy the buffer content into the ns-3
      // packet
      // Do not copy:
      // virtio-net header (12 bytes)
      // the ethernet header (14 next bytes)
      // The IP header (20 next bytes)
      const uint8_t* buffer = m->data + 46;

      ns3::Ptr<ns3::Packet> p = ns3::Create<ns3::Packet>(buffer, m->size - 46);

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
        LOG("Destination address " << m->dst.c_str() << "not found!");
      } else {

        ns3::Ipv4Address dest_ipv4_address = ns3::Ipv4Address::ConvertFrom(tansiv_addresses[pos_dst]);
        ns3::Ipv4Address src_ipv4_address  = ns3::Ipv4Address::ConvertFrom(tansiv_addresses[pos_src]);
        ipHeader.SetDestination(dest_ipv4_address);
        ipHeader.SetSource(src_ipv4_address);
        ipHeader.SetTtl(m->data[26 + 8]);      // TTL is the 9th byte of the ip header
        ipHeader.SetProtocol(m->data[26 + 9]); // Protocol is the 10th byte of the ip header
        ipHeader.SetPayloadSize(p->GetSize());
        p->AddHeader(ipHeader);

        pending_packets.push_back(p);
        pending_messages.push_back(m);

        LOG("Inserting message from " << m->src.c_str() << " to " << m->dst.c_str());
        tansiv_actors[pos_src]->Send(p, tansiv_addresses[pos_dst], 0x0800);
      }
    }

    // if deadline = infinity, then (1) there is only one remaining VM, and (2) it stops its execution
    // so we do not have to sleep until "infinity" because the simulation is done
    if (deadline != std::numeric_limits<double>::infinity()) {
      ns3::Simulator::Stop(ns3::Time::FromDouble(deadline, ns3::Time::S) + ns3::NanoSeconds(1) - ns3::Simulator::Now());
      ns3::Simulator::Run();
    }
    while (ready_to_deliver.size() > 0) {
      vsg::Message* m = ready_to_deliver.back();
      ready_to_deliver.pop_back();

      LOG("[coordinator]: delivering data from vm [" << m->src << "] to vm [" << m->dst << "] (size=" << m->size
                                                     << ")");
      vms_interface->deliverMessage(m);
    }
    LOG("Timestep finished preparing the next iteration [current_time="
        << ns3::Simulator::Now().ToDouble(ns3::Time::S) << "] [next_event = " << get_next_event() << "]");
  }
  vms_interface->end_simulation(true, false);
  LOG("end of simulation");
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
  ns3::Time::SetResolution(ns3::Time::NS);
  ns3::Config::SetDefault ("ns3::RateErrorModel::ErrorRate", ns3::DoubleValue (0));
  ns3::Config::SetDefault ("ns3::BurstErrorModel::ErrorRate", ns3::DoubleValue (0));


  // xbt_assert(argc > 2, "Usage: %s platform_file deployment_file\n", argv[0]);

  std::string socket_name = lookup_args_str("--socket_name", DEFAULT_SOCKET_NAME, argc, argv);

  force_min_latency = lookup_args_double("--force", -1, argc, argv);

  LOG("Forcing the minimum latency to " << force_min_latency);

  vms_interface = new vsg::VmsInterface(socket_name);

  std::string host_name1 = "nova-1.lyon.grid5000.fr";
  std::string host_name2 = "nova-2.lyon.grid5000.fr";
  std::string host_name3 = "nova-3.lyon.grid5000.fr";
  std::string ip1        = "192.168.120.2";
  std::string ip2        = "192.168.120.4";
  std::string ip3        = "192.168.120.6";
  std::string file1      = "boot.py";
  std::string file2      = "boot.py";
  std::string file3      = "boot.py";
  std::string mac1       = "02:ca:fe:f0:0d:0a";
  std::string mac2       = "02:ca:fe:f0:0d:0b";
  std::string mac3       = "02:ca:fe:f0:0d:0c";

  std::vector<std::string> cmd_line1 = {
      "boot.py",
      "--mode",
      "tantap",
      "--num_buffers",
      "100000",
      "--qemu_cmd",
      "tanqemukvm-system-x86_64",
      "--qemu_nictype",
      "virtio-net-pci",
      "--virtio_net_nb_queues",
      "1",
      "--qemu_image",
      "/srv/image.qcow2",
      "--qemu_args",
      "-machine q35,accel=kvm,kernel-irqchip=split -smp sockets=1,cores=1,threads=1,maxcpus=1 -monitor "
      "unix:/tmp/qemu-monitor-1,server,nowait -object "
      "memory-backend-file,size=1M,share=on,mem-path=/dev/shm/ivshmem1,id=hostmem1 -cpu host,invtsc=on -name "
      "debug-threads=on -device intel-iommu,intremap=on,caching-mode=on",
      "--autoconfig_net",
      "192.168.120.2/24",
      "10.0.0.10/24"};

  std::vector<std::string> cmd_line2 = {
      "boot.py",
      "--mode",
      "tantap",
      "--num_buffers",
      "100000",
      "--qemu_cmd",
      "tanqemukvm-system-x86_64",
      "--qemu_nictype",
      "virtio-net-pci",
      "--virtio_net_nb_queues",
      "1",
      "--qemu_image",
      "/srv/image.qcow2",
      "--qemu_args",
      "-machine q35,accel=kvm,kernel-irqchip=split -smp sockets=1,cores=1,threads=1,maxcpus=1 -monitor "
      "unix:/tmp/qemu-monitor-2,server,nowait -object "
      "memory-backend-file,size=1M,share=on,mem-path=/dev/shm/ivshmem1,id=hostmem1 -cpu host,invtsc=on -name "
      "debug-threads=on -device intel-iommu,intremap=on,caching-mode=on",
      "--autoconfig_net",
      "192.168.120.4/24",
      "10.0.0.11/24"};

  std::vector<std::string> cmd_line3 = {
      "boot.py",
      "--mode",
      "tantap",
      "--num_buffers",
      "100000",
      "--qemu_cmd",
      "tanqemukvm-system-x86_64",
      "--qemu_nictype",
      "virtio-net-pci",
      "--virtio_net_nb_queues",
      "1",
      "--qemu_image",
      "/srv/image.qcow2",
      "--qemu_args",
      "-machine q35,accel=kvm,kernel-irqchip=split -smp sockets=1,cores=1,threads=1,maxcpus=1 -monitor "
      "unix:/tmp/qemu-monitor-3,server,nowait -object "
      "memory-backend-file,size=1M,share=on,mem-path=/dev/shm/ivshmem1,id=hostmem1 -cpu host,invtsc=on -name "
      "debug-threads=on -device intel-iommu,intremap=on,caching-mode=on",
      "--autoconfig_net",
      "192.168.120.6/24",
      "10.0.0.12/24"};

  LOG("Set default queue size");
  // Global
  ns3::Config::SetDefault(
      "ns3::DropTailQueue<Packet>::MaxSize",
      ns3::QueueSizeValue(ns3::QueueSize(ns3::QueueSizeUnit::PACKETS, 100))); // ns-3 supports either bytes or packets

  LOG("Build topology.");
  int nSpokes = 3;

  ns3::NodeContainer nodes;
  nodes.Create(nSpokes);

  ns3::PointToPointHelper pointToPoint;
  pointToPoint.SetDeviceAttribute("DataRate", ns3::StringValue(bandwidth));
  // USe MTU > 1500 to avoid having packets splitted by ns-3
  pointToPoint.SetDeviceAttribute("Mtu", ns3::UintegerValue(3000));
  pointToPoint.SetChannelAttribute("Delay", ns3::StringValue(latency));
  pointToPoint.DisableFlowControl();

  // Build the star topology by hand
  ns3::NodeContainer hub;
  ns3::NodeContainer spokes;
  hub.Create(1);
  spokes.Create(nSpokes);

  ns3::NetDeviceContainer spokes_devices;
  ns3::NetDeviceContainer hub_devices;
  for (auto i = 0; i < nSpokes; i++) {
    ns3::NetDeviceContainer nd = pointToPoint.Install(hub.Get(0), spokes.Get(i));
    hub_devices.Add(nd.Get(0));
    spokes_devices.Add(nd.Get(1));
  }

  ns3::InternetStackHelper internet;
  internet.Install(hub);
  internet.Install(spokes);

  LOG("Assign IP Addresses.");
  ns3::Ipv4InterfaceContainer hub_interfaces;
  ns3::Ipv4InterfaceContainer spokes_interfaces;
  ns3::Ipv4AddressHelper address;
  address.SetBase("192.168.120.0", "255.255.255.0");
  for (auto i = 0; i < nSpokes; i++) {
    hub_interfaces.Add(address.Assign(hub_devices.Get(i)));
    spokes_interfaces.Add(address.Assign(spokes_devices.Get(i)));
  }

  ns3::Ptr<ns3::PointToPointNetDevice> net_device1 = ns3::StaticCast<ns3::PointToPointNetDevice>(spokes_devices.Get(0));
  ns3::Ptr<ns3::PointToPointNetDevice> net_device2 = ns3::StaticCast<ns3::PointToPointNetDevice>(spokes_devices.Get(1));
  ns3::Ptr<ns3::PointToPointNetDevice> net_device3 = ns3::StaticCast<ns3::PointToPointNetDevice>(spokes_devices.Get(2));

  ns3::Ptr<ns3::PointToPointNetDevice> net_device_hub1 =
      ns3::StaticCast<ns3::PointToPointNetDevice>(hub_devices.Get(0));
  ns3::Ptr<ns3::PointToPointNetDevice> net_device_hub2 =
      ns3::StaticCast<ns3::PointToPointNetDevice>(hub_devices.Get(1));
  ns3::Ptr<ns3::PointToPointNetDevice> net_device_hub3 =
      ns3::StaticCast<ns3::PointToPointNetDevice>(hub_devices.Get(2));

  ns3::Address mac_address1 = ns3::Mac48Address(mac1.c_str());
  ns3::Address mac_address2 = ns3::Mac48Address(mac2.c_str());
  ns3::Address mac_address3 = ns3::Mac48Address(mac3.c_str());

  net_device1->SetAddress(mac_address1);
  net_device2->SetAddress(mac_address2);
  net_device3->SetAddress(mac_address3);

  /* Inter-packet Gap
   * In ns-3, we send IP packets with a PPP Header
   * We use the InterFrameGap to compensate the PPP header and add the ethernet delays
   * PPP header (-2 bytes)
   * Ethernet Preamble (+8 bytes)
   * Ethernet MAC (+14 bytes )
   * Ethernet CRC (+4 bytes)
   * Ethernet IFG (+12 bytes)
   * Total: +36 bytes
   */
  net_device1->SetInterframeGap(ns3::Time::FromDouble((36 * 8) / 100e6, ns3::Time::S));
  net_device2->SetInterframeGap(ns3::Time::FromDouble((36 * 8) / 100e6, ns3::Time::S));
  net_device3->SetInterframeGap(ns3::Time::FromDouble((36 * 8) / 100e6, ns3::Time::S));

  net_device_hub1->SetInterframeGap(ns3::Time::FromDouble((36 * 8) / 100e6, ns3::Time::S));
  net_device_hub2->SetInterframeGap(ns3::Time::FromDouble((36 * 8) / 100e6, ns3::Time::S));
  net_device_hub3->SetInterframeGap(ns3::Time::FromDouble((36 * 8) / 100e6, ns3::Time::S));

  ns3::Address address1 = spokes_interfaces.GetAddress(0);
  ns3::Address address2 = spokes_interfaces.GetAddress(1);
  ns3::Address address3 = spokes_interfaces.GetAddress(2);

  LOG("Address is " << address1);
  LOG("Address is " << address2);
  LOG("Address is " << address3);

  ns3::Ipv4GlobalRoutingHelper::PopulateRoutingTables();

  // Set infinite TX queues on hosts
  ns3::QueueSizeValue queue_size = ns3::QueueSize(ns3::QueueSizeUnit::PACKETS, UINT32_MAX);
  ns3::PointerValue ptr1;
  ns3::PointerValue ptr2;
  ns3::PointerValue ptr3;

  net_device1->GetAttribute("TxQueue", ptr1);
  net_device2->GetAttribute("TxQueue", ptr2);
  net_device3->GetAttribute("TxQueue", ptr3);

  ns3::Ptr<ns3::Queue<ns3::Packet>> txQueue1 = ptr1.Get<ns3::Queue<ns3::Packet>>();
  ns3::Ptr<ns3::Queue<ns3::Packet>> txQueue2 = ptr2.Get<ns3::Queue<ns3::Packet>>();
  ns3::Ptr<ns3::Queue<ns3::Packet>> txQueue3 = ptr3.Get<ns3::Queue<ns3::Packet>>();

  ns3::Ptr<ns3::DropTailQueue<ns3::Packet>> dtq1 = txQueue1->GetObject<ns3::DropTailQueue<ns3::Packet>>();
  ns3::Ptr<ns3::DropTailQueue<ns3::Packet>> dtq2 = txQueue2->GetObject<ns3::DropTailQueue<ns3::Packet>>();
  ns3::Ptr<ns3::DropTailQueue<ns3::Packet>> dtq3 = txQueue3->GetObject<ns3::DropTailQueue<ns3::Packet>>();

  dtq1->SetAttribute("MaxSize", queue_size);
  dtq2->SetAttribute("MaxSize", queue_size);
  dtq3->SetAttribute("MaxSize", queue_size);

  // pointToPoint.EnablePcapAll("star");

  tansiv_actor(net_device1, address1, mac_address1, host_name1, ip1, file1, cmd_line1);
  tansiv_actor(net_device2, address2, mac_address2, host_name2, ip2, file2, cmd_line2);
  tansiv_actor(net_device3, address3, mac_address3, host_name3, ip3, file3, cmd_line3);

  vm_coordinator();

  delete vms_interface;

  return 0;
}
