#include <atomic>
#include <cstring>
#include <stdio.h>
#include <stdlib.h>
#include <string>
extern "C" {
#include <fake_vm.h>
#include <vsg.h>
}

using namespace std;

// Addresses used in this program
#define ADDR_FMT "10.0.%d.1"

std::atomic<bool> callback_called(false);

void die(const char* msg, int error)
{
  fprintf(stderr, "%s", msg);
  if (error)
    fprintf(stderr, "\t%s\n", std::strerror(error));
  exit(1);
}

// addr must point at at least INET_ADDRSTRLEN chars.
void make_addr(char* addr, int id)
{
  if (snprintf(addr, INET_ADDRSTRLEN, ADDR_FMT, id) >= INET_ADDRSTRLEN) {
    die("Invalid address template or id", 0);
  }
}

void recv_cb(const struct vsg_context* context, uint32_t msglen, const uint8_t* msg)
{
  callback_called = true;
};

int main(int argc, char* argv[])
{
  // initialization phase
  int dest_id                  = std::atoi(argv[1]);
  int src_id                   = 1 - dest_id;
  char dest_str[INET_ADDRSTRLEN];
  make_addr(dest_str, dest_id);
  char src_str[INET_ADDRSTRLEN];
  make_addr(src_str, src_id);
  uint32_t dest                = inet_addr(dest_str);
  uint32_t src                 = inet_addr(src_str);
  int vsg_argc                 = 6;
  const char* const vsg_argv[] = {"-a", CONNECTION_SOCKET_NAME, "-n", src_str, "-t", "1970-01-01T00:00:00"};
  vsg_context* context         = vsg_init(vsg_argc, vsg_argv, NULL, recv_cb);

  if (!context) {
    die("Unable to initialize the context", 0);
  }

  int ret = vsg_start(context);
  if (ret) {
    die("Unable to start the vsg client", ret);
  }

  std::string msg = "plop";
  ret             = vsg_send(context, src, dest, msg.length() + 1, (uint8_t*)msg.c_str());
  if (ret) {
    die("vsg_send() failed", ret);
  }

  // yes, ...
  while (!callback_called.load()) {
  }
  exit(0);

  // vsg_stop block until stopped flag is set
  // stopped flag is set, for instance, when EndSimulation is received
  // but it's likely not going to happen here
  // vsg_stop(context);
  // vsg_cleanup(context);

  return 0;
}
