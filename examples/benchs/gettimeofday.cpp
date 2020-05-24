#include <atomic>
#include <cstring>
#include <limits>
#include <stdio.h>
#include <stdlib.h>
#include <string>
#include <sys/time.h>
extern "C" {
#include <fake_vm.h>
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

vsg_context* init_vsg(int argc, char* argv[])
{
  std::string src_str          = "10.0.0.1";
  uint32_t src                 = inet_addr(src_str.c_str());
  int vsg_argc                 = 6;
  const char* const vsg_argv[] = {"-a", CONNECTION_SOCKET_NAME, "-n", src_str.c_str(), "-t", "1970-01-01T00:00:00"};
  std::atomic<bool> callback_called(false);
  vsg_context* context = vsg_init(vsg_argc, vsg_argv, NULL, recv_cb, (uintptr_t)&callback_called);

  if (!context) {
    die("Unable to initialize the context", 0);
  }

  int ret = vsg_start(context);
  if (ret) {
    die("Unable to start the vsg client", ret);
  }
  return context;
}

#define LIMIT 5

int bench_vsg_gettimeofday(int argc, char* argv[])
{
  vsg_context* context = init_vsg(argc, argv);

  timeval limit = {.tv_sec = LIMIT, .tv_usec = 0};
  int loop_count;
  timeval start;
  vsg_gettimeofday(context, &start, NULL);
  timeval current;
  timeval diff;
  for (loop_count = 0; loop_count < std::numeric_limits<int>::max(); loop_count++) {
    vsg_gettimeofday(context, &current, NULL);
    timersub(&current, &start, &diff);
    if (timercmp(&diff, &limit, >=)) {
      break;
    }
  }
  printf("I'm done with bench_vsg_gettimeofday\n");
  return loop_count;
}

int bench_gettimeofday(int argc, char* argv[])
{
  timeval limit = {.tv_sec = LIMIT, .tv_usec = 0};
  int loop_count;
  timeval start;
  gettimeofday(&start, NULL);
  timeval current;
  timeval diff;
  for (loop_count = 0; loop_count < std::numeric_limits<int>::max(); loop_count++) {
    // TODO(msimonin): faire un truc genre une addition
    // à nombre d'itérations fixé
    // compilé en mode release (make RELEASE=0)
    // make install avec prefix connu
    gettimeofday(&current, NULL);
    timersub(&current, &start, &diff);
    if (timercmp(&diff, &limit, >=)) {
      break;
    }
  }
  printf("I'm done with bench_gettimeofday\n");
  return loop_count;
}

/*
 *
 * Run a simple benchmark to see the effect of gettimeofday implementation
 */
int main(int argc, char* argv[])
{
  int count1   = bench_gettimeofday(argc, argv);
  int count2   = bench_vsg_gettimeofday(argc, argv);
  double rate1 = (double)count1 / LIMIT;
  double rate2 = (double)count2 / LIMIT;
  printf("\n");
  printf("|%-20s|%16.2f /s|\n", "gettimeofday", rate1);
  printf("|%-20s|%16.2f /s|\n", "vsg_gettimeofday", rate2);
  printf("\n");
  exit(0);
}
