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

class Message {
public:
  Message(vsg_send_packet send_packet, uint8_t* payload);
  Message(const Message& other);
  Message(Message&& other);
  Message& operator=(Message&& other);
  ~Message();
  double sent_time;
  vsg_send_packet send_packet;
  // decoded attribute
  std::string src;
  std::string dest;
  // keep size here for backward compatibility
  uint32_t size;
  // this will be dynamically allocated according to size
  uint8_t* data;
};

class VmsInterface {

public:
  VmsInterface(std::string connection_socket_name, bool stop_condition = false);
  ~VmsInterface();
  bool vmActive();
  std::vector<Message*> goTo(double deadline);
  std::string getHostOfVm(std::string vm_name);
  void deliverMessage(Message* m);
  void end_simulation(bool must_unlink = true, bool must_exit = true);
  void register_vm(std::string host_name, std::string vm_name, std::string file, std::vector<std::string> args);
  const std::vector<std::string> get_dead_vm_hosts();

private:
  bool all_vm_active;
  bool a_vm_stopped;
  bool simulate_until_any_stop;

  std::string socket_name;
  int connection_socket;

  std::unordered_map<std::string, int> vm_sockets;
  std::vector<std::string> vm_sockets_trash;
  std::unordered_map<std::string, std::string> vm_deployments; // VM_name |-> host name

  void close_vm_socket(std::string vm_name);
};

} // namespace vsg
