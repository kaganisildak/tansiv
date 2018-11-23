#include "vsg.h"
#include <vector>
#include <string>
#include <unordered_map>

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
    virtual bool vmActive() = 0;
    virtual std::vector<message> goTo(double deadline) = 0; 
    virtual std::string getHostOfVm(std::string vm_name) = 0;
    virtual void deliverMessage(message m) = 0;
};

class DummyVmsInterface: public VmsInterface{
  
  private:
    const int message_size = 100;

    std::unordered_map<std::string, std::string> dest_of_vm;
    int nb_packet_send;

    std::vector<std::string> vm_names;
    std::unordered_map<std::string, std::string> host_of_vms;
    std::unordered_map<std::string, std::vector<double>> vms_sending_times;
  
  public:
    DummyVmsInterface();
    ~DummyVmsInterface();

    bool vmActive();
    std::vector<message> goTo(double deadline);
    std::string getHostOfVm(std::string vm_name);
    void deliverMessage(message m); 
};

}
