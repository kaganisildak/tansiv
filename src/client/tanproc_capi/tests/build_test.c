#include <errno.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <tansiv-client.h>
#include <time.h>

void recv_cb(uintptr_t arg)
{
  int *flag = (int *)arg;
  *flag = true;
}

void deadline_cb(uintptr_t arg, struct timespec deadline)
{
    struct timespec *recorded_deadline = (struct timespec *)arg;
    *recorded_deadline = deadline;
}

void poll_send_cb(uintptr_t arg)
{
}

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
  struct timespec offset;
  struct timeval time;
  int flag = false;
  struct timespec deadline;
  unsigned char msg[] = "Foo msg";
  int res;

  context = vsg_init(argc, argv, NULL, recv_cb, (uintptr_t)&flag, deadline_cb, (uintptr_t)&deadline, poll_send_cb, 0);
  if (!context)
    die("vsg_init() failed", 0);

  res = vsg_start(context, &offset);
  if (res)
    die("vsg_start() failed", res);

  res = vsg_gettimeofday(context, &time, NULL);
  if (res)
    die("vsg_gettimeofday() failed", res);

  uint32_t dest = 1;
  res           = vsg_send(context, dest, sizeof(msg), msg);
  if (res)
    die("vsg_send() failed", res);

  while ((res = vsg_poll(context)) == EAGAIN) {}
  if (res)
    die("vsg_poll() failed", res);

  uint32_t src  = 0;
  uint32_t msglen = sizeof(msg);
  res             = vsg_recv(context, &src, &dest, &msglen, msg);
  if (res)
    die("vsg_recv() failed", res);

  res = vsg_stop(context);
  if (res)
    die("vsg_stop() failed", res);

  vsg_cleanup(context);

  return 0;
}
