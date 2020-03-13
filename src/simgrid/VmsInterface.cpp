#include "VmsInterface.hpp"
#include <xbt/log.hpp>
#include <unistd.h>
#include <signal.h>
#include <algorithm>
#include <arpa/inet.h>

extern "C"
{
#include "vsg.h"
}

XBT_LOG_NEW_DEFAULT_CATEGORY(vm_interface, "Logging specific to the VmsInterface");

namespace vsg
{

bool sortMessages(message i, message j)
{
  return i.sent_time < j.sent_time;
}

vsg_time simgridToVmTime(double simgrid_time)
{
  struct vsg_time vm_time;

  // when only one VM remains, simgrid_time = DOUBLE_MAX.
  // we use the code below to avoid conversion issue of DOUBLE_MAX to uint64_t.
  if (simgrid_time > std::numeric_limits<uint64_t>::max())
  {
    vm_time.seconds = std::numeric_limits<uint64_t>::max();
    vm_time.useconds = std::numeric_limits<uint64_t>::max();
  }
  else
  {
    // the simgrid time correspond to a double in second, so the number of seconds is the integer part
    vm_time.seconds = (uint64_t)(std::floor(simgrid_time));
    // and the number of usecond is the decimal number scaled accordingly
    vm_time.useconds = (uint64_t)(std::floor((simgrid_time - std::floor(simgrid_time)) * 1e6));
  }

  return vm_time;
}

double vmToSimgridTime(vsg_time vm_time)
{
  return vm_time.seconds + (vm_time.useconds * 1e-6);
}

VmsInterface::VmsInterface(bool stop_at_any_stop)
{
  vsg_init();
  a_vm_stopped = false;
  simulate_until_any_stop = stop_at_any_stop;

  connection_socket = socket(PF_LOCAL, SOCK_STREAM, 0);
  XBT_INFO("socket created");

  struct sockaddr_un address;
  address.sun_family = AF_LOCAL;
  strcpy(address.sun_path, CONNECTION_SOCKET_NAME);

  if (bind(connection_socket, (sockaddr *)(&address), sizeof(address)) != 0)
  {
    std::perror("unable to bind connection socket");
    end_simulation();
  }
  XBT_VERB("socket binded");

  if (listen(connection_socket, 1) != 0)
  {
    std::perror("unable to listen on connection socket");
    end_simulation();
  }
  XBT_VERB("listen on socket");

  signal(SIGPIPE, SIG_IGN);
}

VmsInterface::~VmsInterface()
{
  end_simulation(true, false);
}

void VmsInterface::register_vm(std::string host_name, std::string vm_name, std::string file,
                               std::vector<std::string> argv)
{

  vm_deployments[vm_name] = host_name;

  std::vector<char *> command;
  std::string exec_line = file;
  for (auto const &arg : argv)
  {
    command.emplace_back(const_cast<char *>(arg.c_str()));
    exec_line += " " + arg;
  }
  command.push_back(nullptr);

  XBT_INFO("fork and exec of [%s]", exec_line.c_str());

  switch (fork())
  {
  case -1:
    std::perror("unable to fork process");
    end_simulation();
    break;
  case 0:
    end_simulation(false, false);

    if (execvp(file.c_str(), command.data()) != 0)
    {
      std::perror("unable to launch VM");
      end_simulation();
    }
    break;
  default:
    break;
  }
  XBT_VERB("fork done for VM %s", vm_name.c_str());

  struct sockaddr_un vm_address = {0};
  unsigned int len = sizeof(vm_address);
  int vm_socket = accept(connection_socket, (sockaddr *)(&vm_address), &len);
  if (vm_socket < 0)
    std::perror("unable to accept connection on socket");

  vm_sockets[vm_name] = vm_socket;
  XBT_INFO("connection for VM %s established", vm_name.c_str());
}

void VmsInterface::end_simulation(bool must_unlink, bool must_exit)
{
  close(connection_socket);
  for (auto it : vm_sockets)
  {
    close(it.second);
  }
  XBT_VERB("vm sockets are down");

  if (must_unlink)
    unlink(CONNECTION_SOCKET_NAME);

  if (must_exit)
  {
    exit(666);
  }
}

bool VmsInterface::vmActive()
{
  return (!vm_sockets.empty() && !simulate_until_any_stop) || (!a_vm_stopped && simulate_until_any_stop);
}

std::vector<message> VmsInterface::goTo(double deadline)
{
  // Beforehand, forget about the VMs that bailed out recently.
  // We hope that the coordinator cleaned the SimGrid side in between
  vm_sockets_trash.clear();

  // first, we ask all the VMs to go to deadline
  XBT_DEBUG("Sending: go to deadline %f (%f)", deadline, vmToSimgridTime(simgridToVmTime(deadline)));
  uint32_t goto_flag = vsg_msg_from_actor_type::VSG_GO_TO_DEADLINE;
  struct vsg_time vm_deadline = simgridToVmTime(deadline);

  for (auto it : vm_sockets)
  {
    //vsg_goto_deadline_send(it.second, vm_deadline);
    send(it.second, &goto_flag, sizeof(uint32_t), 0);
    send(it.second, &vm_deadline, sizeof(vsg_time), 0);
  }

  // then, we pick up all the messages send by the VM until they reach the deadline
  std::vector<message> messages;
  XBT_DEBUG("getting the message send by the VMs");
  for (auto kv : vm_sockets)
  {
    uint32_t vm_flag = 0;
    std::string vm_name = kv.first;
    int vm_socket = kv.second;

    while (true)
    {

      // ->vsg_order_recv
      if (recv(vm_socket, &vm_flag, sizeof(uint32_t), MSG_WAITALL) <= 0)
      {
        XBT_INFO("can not receive the flags of VM %s. Forget about the socket that seem closed at the system level.",
                 vm_name.c_str());
        close_vm_socket(vm_name);
      }

      // When the VM reaches the deadline, we're done with it, let's consider the next VM
      if (vm_flag == vsg_msg_to_actor_type::VSG_AT_DEADLINE)
      {
        break;
      }
      else if (vm_flag == vsg_msg_to_actor_type::VSG_SEND_PACKET)
      {

        XBT_VERB("getting a message from VM %s", vm_name.c_str());
        struct vsg_send_packet packet = {0};
        // we first get the message size
        recv(vm_socket, &packet, sizeof(packet), MSG_WAITALL);
        // then we get the message itself and we split
        //   - the destination address (first part) that is only useful for setting up the communication in SimGrid
        //   - and the data transfer, that correspond to the data actually send through the (simulated) network
        // (nb: we use vm_name.length() to determine the size of the destination address because we assume all the vm id
        // to have the same size)
        char data[packet.packet.size];
        if (recv(vm_socket, data, sizeof(data), MSG_WAITALL) <= 0)
        {
          XBT_ERROR("can not receive the data of the message from VM %s. The socket may be closed", vm_name.c_str());
          end_simulation();
        }
        struct in_addr _dest = {packet.packet.dest.addr};
        XBT_INFO("got the message [%s] (size %lu) from VM [%s] to VM [%s] with timestamp [%d.%d]",
                 data, sizeof(data), vm_name.c_str(), inet_ntoa(_dest), packet.send_time.seconds, packet.send_time.useconds);

        struct message m;
        // NB: packet_size is the size used by SimGrid to simulate the transfer of the data on the network.
        //     It does NOT correspond to the size of the data transfered to/from the VM on the REAL socket.
        m.packet_size = sizeof(data);
        m.data.append(data);
        m.src = vm_name;
        m.dest.append(inet_ntoa(_dest));
        m.sent_time = vmToSimgridTime(packet.send_time);
        messages.push_back(m);
      }
      else
      {
        XBT_ERROR("unknown message received from VM %s : %lu", vm_name.c_str(), vm_flag);
        end_simulation();
      }
    }
  }
  // Remove all invalid sockets from our list, but leave a chance to the coordinator to notice about them
  for (auto sock_name : vm_sockets_trash)
    vm_sockets.erase(sock_name);

  XBT_DEBUG("forwarding all the messages to SimGrid");

  std::sort(messages.begin(), messages.end(), sortMessages);

  return messages;
}

std::string VmsInterface::getHostOfVm(std::string vm_name)
{
  if (vm_deployments.find(vm_name) == vm_deployments.end())
  {
    return "";
  }
  return vm_deployments[vm_name];
}
void VmsInterface::close_vm_socket(std::string vm_name)
{
  int vm_socket = vm_sockets.at(vm_name);
  shutdown(vm_socket, SHUT_RDWR);
  close(vm_socket);
  vm_sockets_trash.push_back(vm_name);
  a_vm_stopped = true;
}
const std::vector<std::string> VmsInterface::get_dead_vm_hosts()
{
  std::vector<std::string> dead_hosts;
  for (std::string vm : vm_sockets_trash)
  {
    dead_hosts.push_back(getHostOfVm(vm));
  }

  return dead_hosts;
}

void VmsInterface::deliverMessage(message m)
{

  if (vm_sockets.find(m.dest) != vm_sockets.end())
  {
    int socket = vm_sockets[m.dest];
    uint32_t deliver_flag = vsg_msg_from_actor_type::VSG_DELIVER_PACKET;
    std::string data = m.data;
    // TODO(msimonin): Store the whole packet structure...
    struct vsg_addr dest = {inet_addr(m.dest.c_str()), 0};
    struct vsg_addr src = {inet_addr(m.src.c_str()), 0};
    struct vsg_packet packet = {
        .size = data.length(),
        .dest = dest,
        .src = src};
    struct vsg_deliver_packet deliver_packet = {
        packet = packet};
    vsg_deliver_send(socket, deliver_packet, data.c_str());

    XBT_VERB("message from vm %s delivered to vm %s", m.src.c_str(), m.dest.c_str());
  }
  else
  {
    XBT_WARN("message from vm %s was not delivered to vm %s because it already stopped its execution", m.src.c_str(),
             m.dest.c_str());
  }
}

} // namespace vsg
