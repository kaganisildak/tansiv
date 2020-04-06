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

std::atomic<bool> callback_called(false);

void die(const char* msg, int error)
{
  fprintf(stderr, "%s", msg);
  if (error)
    fprintf(stderr, "\t%s\n", std::strerror(error));
  exit(1);
}

void recv_cb(const struct vsg_context* context, uint32_t msglen, const uint8_t* msg)
{
  callback_called = true;
};

int main(int argc, char* argv[])
{
  // initialization phase
  uint32_t dest                = std::atoi(argv[1]);
  uint32_t src                 = 1 - dest;
  int vsg_argc                 = 4;
  const char* const vsg_argv[] = {"-a", CONNECTION_SOCKET_NAME, "-t", "1970-01-01T00:00:00"};
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
