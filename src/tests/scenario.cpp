#include "scenario.hpp"
#include "catch.hpp"

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
  exit(0);
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
  }
  // Start queueing incoming connections otherwise there might be a race
  // condition where vsg_init is called before the server side socket is
  // listening.
  if (listen(connection_socket, 1) != 0) {
    std::perror("unable to listen on connection socket");
  }
  printf("Actor is now ready to listen to connections\n");

  pid_t pid = fork();
  if (pid == 0) {
    // Adding a signal to leave the child gracefully
    // when the test ends
    signal(SIGQUIT, sigquit);
    // child process: we continue the actor execution
    struct sockaddr_un vm_address = {0};
    unsigned int len              = sizeof(vm_address);
    printf("\tWaiting connections\n");
    int client_socket = accept(connection_socket, (sockaddr*)(&vm_address), &len);
    if (client_socket < 0)
      std::perror("unable to accept connection on socket");
    printf("\tClient connection accepted\n");
    // run it
    (*the_scenario)(client_socket);
  } else if (pid > 0) {
    // sets the attributes
    printf("I'm your father (my child=%d)\n", pid);
    this->child_pid = pid;
    this->vsg_fd    = connection_socket;
  } else {
    exit(1);
  }
}

ScenarioRunner::~ScenarioRunner()
{
  printf("Closing the socket %d\n", this->vsg_fd);
  close(this->vsg_fd);
  /* Terminate child. */
  printf("Terminating %d \n", this->child_pid);
  pid_t pid = this->child_pid;
  kill(pid, SIGQUIT);
  int status;
  waitpid(pid, &status, 0);

  /* Report an error. */
  if (WIFEXITED(status) && WEXITSTATUS(status) > 0) {
    printf("status=%d\n", WEXITSTATUS(status));
    // FIXME, do something
  }
}

static void init_sequence(int client_socket)
{
  vsg_msg_in_type msg = vsg_msg_in_type::GoToDeadline;
  int ret             = send(client_socket, &msg, sizeof(uint32_t), 0);
  vsg_time t          = {0, 200};
  send(client_socket, &t, sizeof(vsg_time), 0);
}

static void end_sequence(int client_socket)
{
  vsg_msg_in_type msg = vsg_msg_in_type::EndSimulation;
  send(client_socket, &msg, sizeof(uint32_t), 0);
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
  printf("Entering recv_one scenario\n");
  init_sequence(client_socket);

  // first, check the type of message
  vsg_msg_out_type msg_type;
  recv(client_socket, &msg_type, sizeof(vsg_msg_out_type), MSG_WAITALL);
  REQUIRE(vsg_msg_out_type::SendPacket == msg_type);

  // second, check the send time and size
  vsg_send_packet send_packet = {0};
  recv(client_socket, &send_packet, sizeof(vsg_send_packet), MSG_WAITALL);

  // test the received addresses
  in_addr_t src_expected = inet_addr(SRC);
  REQUIRE(src_expected == send_packet.packet.src);

  in_addr_t dst_expected = inet_addr(DEST);
  REQUIRE(dst_expected == send_packet.packet.dst);

  // finally get the payload
  uint8_t buf[send_packet.packet.size];
  recv(client_socket, buf, send_packet.packet.size, MSG_WAITALL);

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
  vsg_deliver_send(client_socket, deliver_packet, (uint8_t*)data.c_str());
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
  printf("Entering send_deliver_pg_port scenario\n");
  init_sequence(client_socket);

  // receive send_packet
  vsg_msg_out_type msg_type;
  recv(client_socket, &msg_type, sizeof(vsg_msg_out_type), MSG_WAITALL);
  vsg_send_packet send_packet = {0};
  recv(client_socket, &send_packet, sizeof(vsg_send_packet), MSG_WAITALL);
  // here the payload contains the port
  // we just pass it back to the app with a deliver message
  uint8_t buf[send_packet.packet.size];
  recv(client_socket, buf, send_packet.packet.size, MSG_WAITALL);

  // deliver sequence
  uint32_t deliver_flag                    = vsg_msg_in_type::DeliverPacket;
  vsg_packet packet                        = {.size = sizeof(buf)};
  struct vsg_deliver_packet deliver_packet = {.packet = packet};
  vsg_deliver_send(client_socket, deliver_packet, buf);

  end_sequence(client_socket);
  printf("Leaving send_deliver_pg_port scenario\n");
};
