#include "vsg.h"
#include <vector>
#include <string>

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
    VmsInterface();
    ~VmsInterface();
    
    bool vmActive();
    std::vector<message> goTo(double deadline);
    std::string getHostOfVm(std::string vm_name);
    void deliverMessage(message m);
};

}
