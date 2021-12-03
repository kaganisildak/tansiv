#include <atomic>
#include <cstring>
#include <stdio.h>
#include <stdlib.h>
#include <string>
#include <unistd.h>
extern "C" {
#include <tansiv-client.h>
#include <vsg.h>
}

using namespace std;

// Addresses used in this program
#define ADDR_FMT "10.0.%d.1"

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

void recv_cb(uintptr_t arg)
{
  std::atomic<bool>* callback_called = (std::atomic<bool>*)arg;
  *callback_called                   = true;
};

void deadline_cb(uintptr_t arg, struct timespec deadline) {}

int main(int argc, char* argv[])
{
  // initialization phase
  if (argc < 2) {
    printf("Usage: send socket_name dest_id \n");
  }
  char* socket_name = argv[1];
  int dest_id       = std::atoi(argv[2]);
  printf("socket_name=%s\n", socket_name);
  printf("dest_id=%d\n", dest_id);
  int src_id = 1 - dest_id;
  char dest_str[INET_ADDRSTRLEN];
  make_addr(dest_str, dest_id);
  char src_str[INET_ADDRSTRLEN];
  make_addr(src_str, src_id);
  uint32_t dest                = inet_addr(dest_str);
  uint32_t src                 = inet_addr(src_str);
  int vsg_argc                 = 6;
  const char* const vsg_argv[] = {"-a", socket_name, "-n", src_str, "-t", "1970-01-01T00:00:00"};
  std::atomic<bool> callback_called(false);
  vsg_context* context = vsg_init(vsg_argc, vsg_argv, NULL, recv_cb, (uintptr_t)&callback_called, deadline_cb, 0);

  if (!context) {
    die("Unable to initialize the context", 0);
  }

  int ret = vsg_start(context, NULL);
  if (ret) {
    die("Unable to start the vsg client", ret);
  }

  std::string msg = "plop";
  ret             = vsg_send(context, dest, msg.length() + 1, (uint8_t*)msg.c_str());
  if (ret) {
    die("vsg_send() failed", ret);
  }

  // yes, ...
  while (!callback_called.load()) {
  }

  uint32_t recv_src;
  uint32_t recv_dest;
  uint32_t buffer_len = msg.length() + 1;
  char buffer[buffer_len];
  ret = vsg_recv(context, &recv_src, &recv_dest, &buffer_len, (uint8_t*)buffer);
  if (ret) {
    die("vsg_recv() failed", ret);
  }

  char recv_src_str[INET_ADDRSTRLEN];
  inet_ntop(AF_INET, &recv_src, recv_src_str, INET_ADDRSTRLEN);
  char recv_dest_str[INET_ADDRSTRLEN];
  inet_ntop(AF_INET, &recv_dest, recv_dest_str, INET_ADDRSTRLEN);
  // We trust our peer to have sent the final NUL byte... or we will see that he
  // is a bad boy!
  printf("\n###### \n");
  printf("Received from %s to %s: %s", recv_src_str, recv_dest_str, buffer);
  printf("\n###### \n\n");

  // vsg_stop block until stopped flag is set
  // stopped flag is set, for instance, when EndSimulation is received
  // but it's likely not going to happen here
  // vsg_stop(context);
  // vsg_cleanup(context);

  return 0;
}
