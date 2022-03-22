#ifndef __VMSINTERFACE__
#define __VMSINTERFACE__

#include <arpa/inet.h>
#include <cmath>
#include <string>
#include <sys/socket.h>
#include <sys/un.h>
#include <unordered_map>
#include <vector>

namespace vsg {

struct vsg_time {
  uint64_t seconds;
  uint64_t useconds;
};

class Message {
public:
  Message(uint64_t seconds, uint64_t useconds, in_addr_t src_enc, in_addr_t dst_enc, uint32_t size, uint8_t* payload);
  Message(const Message& other);
  Message(Message&& other);
  Message& operator=(Message&& other);
  ~Message();
  uint64_t seconds;
  u_int64_t useconds;
  in_addr_t src_enc;
  in_addr_t dst_enc;
  uint32_t size;
  // computed attribute below
  double sent_time;
  // decoded attribute
  std::string src;
  std::string dst;
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

#endif