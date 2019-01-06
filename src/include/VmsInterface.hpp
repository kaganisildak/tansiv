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
  std::string data;
  int packet_size;
};

class VmsInterface{

  public:
    VmsInterface(std::string executable_path, std::unordered_map<std::string,std::string> host_of_vms, bool stop_condition = false);
    ~VmsInterface();
    bool vmActive();
    std::vector<message> goTo(double deadline); 
    std::string getHostOfVm(std::string vm_name);
    void deliverMessage(message m);

  private:
    const char* CONNECTION_SOCKET_NAME = "simgrid_connection_socket";
    bool all_vm_active;
    bool a_vm_stopped;
    bool simulate_until_any_stop;
 
    std::unordered_map<std::string, int> vm_sockets;
    std::unordered_map<std::string,std::string> vm_deployments;    

    double vmToSimgridTime(vsg_time vm_time);
    vsg_time simgridToVmTime(double simgrid_time);
    
};

}
