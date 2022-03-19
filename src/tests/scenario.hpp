
#ifndef __SCENARIO__
#define __SCENARIO__
#include <sys/types.h>
#include <arpa/inet.h>
#include <errno.h>

extern "C" {
#include <tansiv-client.h>
#include <vsg.h>
}
#include <packets_generated.h>

/* The socket to use for all the tests. */
#define SOCKET_ACTOR "titi"

/* The message to send for send/deliver tests. */
#define MESSAGE "plop"

/* The source to use for send tests. */
#define SRC "127.0.0.1"

/* The destination to use for send tests. */
#define DEST "8.8.8.8"

typedef void scenario(int);

class ScenarioRunner {
public:
  ScenarioRunner(scenario s);
  ~ScenarioRunner();
  void finalize();

  pid_t child_pid;
  int life_pipe_fd;
};

// the scenarios
void simple(int);
void recv_one(int);
void deliver_one(int);
void send_deliver_pg_port(int);

void fb_simple(int);
void fb_deliver(int);

/*recv flatbuffer message from a socket.

Must provide a big enough buffer...
*/
static int fb_recv(int sock, uint8_t* buffer, size_t buf_size)
{
  char len_buf[4];
  // our fb are prefixed with their size
  int ret  = vsg_protocol_recv(sock, len_buf, 4);
  if (ret) {
      return ret;
  }
  auto len = flatbuffers::ReadScalar<uint8_t>(len_buf);
  if (buf_size < len) {
      errno = ENOBUFS;
      perror("fb_recv");
      fprintf(stderr, "  %zd bytes provided but at least %d bytes required\n", buf_size, len);
      return -1;
  }
  // read the remaining part
  return vsg_protocol_recv(sock, buffer, len);
}


#endif /* __SCENARIO__ */
