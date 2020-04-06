#include <cmath>
#include <string>
#include <sys/socket.h>
#include <sys/un.h>
#include <unordered_map>
#include <vector>
extern "C" {
#include "vsg.h"
}

namespace vsg {

struct message {
  double sent_time;
  std::string src;      // decoded src
  std::string dest;     // decoded dest
  uint32_t packet_size; // decoded packet size
  vsg_packet packet;    // (raw)packet info
  std::string data;     // raw data
};

class VmsInterface {

public:
  VmsInterface(bool stop_condition = false);
  ~VmsInterface();
  bool vmActive();
  std::vector<message> goTo(double deadline);
  std::string getHostOfVm(std::string vm_name);
  void deliverMessage(message m);
  void end_simulation(bool must_unlink = true, bool must_exit = true);
  void register_vm(std::string host_name, std::string vm_name, std::string file, std::vector<std::string> args);
  const std::vector<std::string> get_dead_vm_hosts();

private:
  bool all_vm_active;
  bool a_vm_stopped;
  bool simulate_until_any_stop;

  int connection_socket;

  std::unordered_map<std::string, int> vm_sockets;
  std::vector<std::string> vm_sockets_trash;
  std::unordered_map<std::string, std::string> vm_deployments; // VM_name |-> host name

  void close_vm_socket(std::string vm_name);
};

} // namespace vsg
