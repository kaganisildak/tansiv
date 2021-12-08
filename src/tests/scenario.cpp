#include "catch.hpp"
#include "scenario.hpp"

#include <csignal>
#include <cstdio>
#include <stdio.h>
#include <stdlib.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <sys/wait.h>
#include <unistd.h>

using namespace std;

void sigquit(int signum)
{
  // gracefully leave
  _exit(0);
}

ScenarioRunner::ScenarioRunner(scenario* the_scenario)
{
  remove(SOCKET_ACTOR);
  printf("\n---\nCreating Simple Actor\n");
  int connection_socket = socket(AF_LOCAL, SOCK_STREAM, 0);

  struct sockaddr_un address;
  address.sun_family = AF_LOCAL;
  strcpy(address.sun_path, SOCKET_ACTOR);

  if (bind(connection_socket, (sockaddr*)(&address), sizeof(address)) != 0) {
    std::perror("unable to bind connection socket");
    exit(1);
  }
  // Start queueing incoming connections otherwise there might be a race
  // condition where vsg_init is called before the server side socket is
  // listening.
  if (listen(connection_socket, 1) != 0) {
    std::perror("unable to listen on connection socket");
    exit(1);
  }
  printf("Actor is now ready to listen to connections\n");

  int life_pipe[2];
  if (pipe(life_pipe) != 0) {
    std::perror("unable to create life_pipe\n");
    exit(1);
  }

  pid_t pid = fork();
  if (pid == 0) {
    // Adding a signal to leave the child gracefully
    // when the test ends
    signal(SIGQUIT, sigquit);
    // Close the write end of life_pipe, so that read returns EOF when
    // the parent dies.
    close(life_pipe[1]);

    // child process: we continue the actor execution
    struct sockaddr_un vm_address = {0};
    unsigned int len              = sizeof(vm_address);
    printf("\tWaiting connections\n");
    int client_socket = accept(connection_socket, (sockaddr*)(&vm_address), &len);
    if (client_socket < 0) {
      std::perror("unable to accept connection on socket");
      exit(1);
    }
    printf("\tClient connection accepted\n");
    // run it
    (*the_scenario)(client_socket);

    // Wait for the parent to (abnormally) exit or (normally) terminate us by SIGQUIT
    char dummy;
    read(life_pipe[0], &dummy, 1);
    exit(0);
  } else if (pid > 0) {
    // Parent: close now unused fds
    close(connection_socket);
    close(life_pipe[0]);
    // sets the attributes
    printf("I'm your father (my child=%d)\n", pid);
    this->child_pid = pid;
    this->life_pipe_fd = life_pipe[1];
  } else {
    exit(1);
  }
}

ScenarioRunner::~ScenarioRunner()
{
  /* Terminate child. */
  printf("Terminating %d \n", this->child_pid);
  pid_t pid = this->child_pid;
  kill(pid, SIGQUIT);
  int status;
  waitpid(pid, &status, 0);

  /* Not mandatory to terminate child but avoids fd leaks */
  close(this->life_pipe_fd);

  /* Report an error. */
  if (WIFEXITED(status) && WEXITSTATUS(status) > 0) {
    printf("status=%d\n", WEXITSTATUS(status));
    exit(1);
  }
}

// Hard-coded time slice of 200 microseconds
static void send_go_to_deadline(int client_socket)
{
  int ret;

  vsg_msg_in_type msg = vsg_msg_in_type::GoToDeadline;
  ret                 = vsg_protocol_send(client_socket, &msg, sizeof(uint32_t));
  REQUIRE(0 == ret);
  vsg_time t = {0, 200};
  ret        = vsg_protocol_send(client_socket, &t, sizeof(vsg_time));
  REQUIRE(0 == ret);
}

static void init_sequence(int client_socket)
{
  send_go_to_deadline(client_socket);
}

static void end_sequence(int client_socket)
{
  vsg_msg_in_type msg = vsg_msg_in_type::EndSimulation;
  int ret             = vsg_protocol_send(client_socket, &msg, sizeof(uint32_t));
  REQUIRE(0 == ret);
}

/*
 * Simple scenario
 *
 * The actor sends the init_sequence:
 *  - a GoToDeadline message
 *  - an EndSimulation message
 *
 */
void simple(int client_socket)
{
  printf("Entering simple scenario\n");
  init_sequence(client_socket);
  end_sequence(client_socket);
  printf("Leaving simple scenario\n");
}

/*
 * scenario: recv_one
 *
 * The actor sends
 *  - the init sequence
 *  - wait a message sent by the application
 *
 */
void recv_one(int client_socket)
{
  int ret;

  printf("Entering recv_one scenario\n");
  init_sequence(client_socket);

  vsg_msg_out_type msg_type;
  do {
    // AtDeadline msgs can arrive before getting the first SendPacket
    // https://gitlab.inria.fr/tansiv/tansiv/-/issues/20
    // so we loop until we get somethiing different than AtDeadline
    // and this message must be a SendPacket
    ret = vsg_protocol_recv(client_socket, &msg_type, sizeof(vsg_msg_out_type));
    REQUIRE(0 == ret);
    send_go_to_deadline(client_socket);
  } while (msg_type == vsg_msg_out_type::AtDeadline);

  REQUIRE(vsg_msg_out_type::SendPacket == msg_type);

  // second, check the send time and size
  vsg_send_packet send_packet = {0};
  ret                         = vsg_protocol_recv(client_socket, &send_packet, sizeof(vsg_send_packet));
  REQUIRE(0 == ret);

  // test the received addresses
  in_addr_t src_expected = inet_addr(SRC);
  REQUIRE(src_expected == send_packet.packet.src);

  in_addr_t dst_expected = inet_addr(DEST);
  REQUIRE(dst_expected == send_packet.packet.dst);

  // finally get the payload
  uint8_t buf[send_packet.packet.size];
  ret = vsg_protocol_recv(client_socket, buf, send_packet.packet.size);
  REQUIRE(0 == ret);

  std::string expected = MESSAGE;
  std::string actual   = std::string((char*)buf);
  REQUIRE(expected == actual);

  end_sequence(client_socket);
  printf("Leaving recv_one scenario\n");
};

/*
 * scenario: deliver_one
 *
 * The actor sends
 *  - the init sequence
 *  - send a DeliverPacket to the application
 *
 */
void deliver_one(int client_socket)
{
  printf("Entering deliver_one scenario\n");
  init_sequence(client_socket);

  uint32_t deliver_flag = vsg_msg_in_type::DeliverPacket;
  std::string data      = MESSAGE;
  vsg_packet packet     = {.size = (uint32_t)data.length() + 1, .src = inet_addr(SRC), .dst = inet_addr(DEST)};
  struct vsg_deliver_packet deliver_packet = {.packet = packet};
  int ret                                  = vsg_deliver_send(client_socket, deliver_packet, (uint8_t*)data.c_str());
  REQUIRE(0 == ret);
  printf("Leaving deliver_one scenario\n");

  end_sequence(client_socket);
};

/*
 * scenario: send_deliver_pg_port
 *
 * The actor sends
 *  - the init sequence
 *  - wait a message sent by the application (with a port piggybacked)
 *
 */
void send_deliver_pg_port(int client_socket)
{
  int ret;

  printf("Entering send_deliver_pg_port scenario\n");
  init_sequence(client_socket);

  // wait for send_packet
  vsg_msg_out_type msg_type;
  do {
    // AtDeadline msgs can arrive before getting the first SendPacket
    // https://gitlab.inria.fr/tansiv/tansiv/-/issues/20
    // so we loop until we get somethiing different than AtDeadline
    // and this message must be a SendPacket
    ret = vsg_protocol_recv(client_socket, &msg_type, sizeof(vsg_msg_out_type));
    REQUIRE(0 == ret);
    send_go_to_deadline(client_socket);
  } while (msg_type == vsg_msg_out_type::AtDeadline);

  REQUIRE(vsg_msg_out_type::SendPacket == msg_type);
  vsg_send_packet send_packet = {0};
  ret                         = vsg_protocol_recv(client_socket, &send_packet, sizeof(vsg_send_packet));
  REQUIRE(0 == ret);

  // here the payload contains the port
  // we just pass it back to the app with a deliver message
  uint8_t buf[send_packet.packet.size];
  ret = vsg_protocol_recv(client_socket, buf, send_packet.packet.size);
  REQUIRE(0 == ret);

  // deliver sequence
  uint32_t deliver_flag                    = vsg_msg_in_type::DeliverPacket;
  vsg_packet packet                        = {.size = (uint32_t)sizeof(buf)};
  struct vsg_deliver_packet deliver_packet = {.packet = packet};
  ret                                      = vsg_deliver_send(client_socket, deliver_packet, buf);
  REQUIRE(0 == ret);

  end_sequence(client_socket);
  printf("Leaving send_deliver_pg_port scenario\n");
};
