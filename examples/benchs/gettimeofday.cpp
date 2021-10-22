#include <atomic>
#include <cstring>
#include <limits>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string>
#include <sys/time.h>
extern "C" {
#include <tansiv-client.h>
#include <vsg.h>
}

using namespace std;

#define MAX_COUNT ((uint64_t)pow(10, 8))

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

void deadline_cb(uintptr_t arg, struct timespec deadline)
{
}

vsg_context* init_vsg(int argc, char* argv[])
{
  std::string src_str          = "10.0.0.1";
  uint32_t src                 = inet_addr(src_str.c_str());
  int vsg_argc                 = 6;
  const char* const vsg_argv[] = {"-a", CONNECTION_SOCKET_NAME, "-n", src_str.c_str(), "-t", "1970-01-01T00:00:00"};
  std::atomic<bool> callback_called(false);
  vsg_context* context = vsg_init(vsg_argc, vsg_argv, NULL, recv_cb, (uintptr_t)&callback_called, deadline_cb, 0);

  if (!context) {
    die("Unable to initialize the context", 0);
  }

  int ret = vsg_start(context, NULL);
  if (ret) {
    die("Unable to start the vsg client", ret);
  }
  return context;
}

#define LIMIT 5

double to_double(timeval time)
{
  return (double)time.tv_sec + ((double)time.tv_usec) * pow(10, -6);
}

double bench_vsg_gettimeofday(int argc, char* argv[])
{
  vsg_context* context = init_vsg(argc, argv);

  timeval start;
  timeval current;
  timeval diff;
  double result = 0.;

  vsg_gettimeofday(context, &start, NULL);
  for (int loop_count = 1; loop_count < MAX_COUNT; loop_count++) {
    result = result + 1 / pow(loop_count, 2);
  }
  vsg_gettimeofday(context, &current, NULL);

  timersub(&current, &start, &diff);
  // printf("vsg_gettimeofday] 6*result = %f\n", result);
  return to_double(diff);
}

double bench_gettimeofday(int argc, char* argv[])
{
  timeval start;
  timeval current;
  timeval diff;
  double result = 0.;

  gettimeofday(&start, NULL);
  for (int loop_count = 1; loop_count < MAX_COUNT; loop_count++) {
    result = result + 1. / pow(loop_count, 2);
  }
  gettimeofday(&current, NULL);

  timersub(&current, &start, &diff);
  // printf("gettimeofday] 6*result = %f\n", result);
  return to_double(diff);
}

/*
 *
 * Run a simple benchmark to see the effect of gettimeofday implementation
 */
int main(int argc, char* argv[])
{
  double time1 = bench_gettimeofday(argc, argv);
  double time2 = bench_vsg_gettimeofday(argc, argv);
  printf("%f, %f\n", time1, time2);
  /*
  printf("\n");
  printf("|%-20s|%16.3f s|\n", "gettimeofday", time1);
  printf("|%-20s|%16.3f s|\n", "vsg_gettimeofday", time2);
  printf("\n");
  */
  exit(0);
}
