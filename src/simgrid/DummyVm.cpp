#include "vsg.h"
#include <vector>
#include <string>
#include <unordered_map>
#include <sys/socket.h>
#include <sys/un.h>
#include <sys/types.h>

int main(int argc, char *argv[])
{
  int vm_socket = socket(PF_LOCAL, SOCK_STREAM, 0);  
  
  struct sockaddr_un address;
  address.sun_family = AF_LOCAL;
  strcpy(address.sun_path, argv[1]);

  if(connect(vm_socket, (sockaddr*)(&address), sizeof(address)) != 0){
    std::perror("unable to create VM socket");   
    exit(666);
  }

  //close(vm_socket);

  return 0;
}
