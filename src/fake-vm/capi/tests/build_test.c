#include <fake_vm.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

void recv_cb(const struct vsg_context* context, uint32_t msglen, const uint8_t* msg) {}

void die(const char* msg, int error)
{
  fprintf(stderr, "%s", msg);
  if (error)
    fprintf(stderr, "\t%s\n", strerror(error));
  exit(1);
}

int main(int argc, const char* argv[])
{
  struct vsg_context* context;
  struct timeval time;
  unsigned char msg[] = "Foo msg";
  int res;

  context = vsg_init(argc, argv, NULL, recv_cb);
  if (!context)
    die("vsg_init() failed", 0);

  res = vsg_start(context);
  if (res)
    die("vsg_start() failed", res);

  res = vsg_gettimeofday(context, &time, NULL);
  if (res)
    die("vsg_gettimeofday() failed", res);

  uint32_t src  = 0;
  uint32_t dest = 1;
  res           = vsg_send(context, src, dest, sizeof(msg), msg);
  if (res)
    die("vsg_send() failed", res);

  res = vsg_stop(context);
  if (res)
    die("vsg_stop() failed", res);

  vsg_cleanup(context);

  return 0;
}
