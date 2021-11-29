
#ifndef __SCENARIO__
#define __SCENARIO__
#include <sys/types.h>

extern "C" {
#include <tansiv-client.h>
#include <vsg.h>
}

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
  int vsg_fd;
  int life_pipe_fd;
};

// the scenarios
void simple(int);
void recv_one(int);
void deliver_one(int);
void send_deliver_pg_port(int);

#endif /* __SCENARIO__ */
