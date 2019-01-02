#include "vsg.h"
#include <vector>
#include <string>
#include <unordered_map>
#include <sys/socket.h>
#include <sys/un.h>

namespace vsg{

struct message{
  double time;
  std::string src;
  std::string dest;
  vsg_packet data;
  int packet_size;
};

class VmsInterface{

  public:
    VmsInterface(std::vector<std::string> vm_names);
    ~VmsInterface();
    bool vmActive();
    std::vector<message> goTo(double deadline); 
    std::string getHostOfVm(std::string vm_name);
    void deliverMessage(message m);

  private:
    std::unordered_map<std::string, int> vm_sockets;
};

}
