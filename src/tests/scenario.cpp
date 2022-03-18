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
  _exit(0);
}

ScenarioRunner::ScenarioRunner(scenario* the_scenario)
{
  remove(SOCKET_ACTOR);
  printf("\n---\n[scenario runner] Creating Simple Actor\n");
  int connection_socket = socket(AF_LOCAL, SOCK_STREAM, 0);

  struct sockaddr_un address;
  address.sun_family = AF_LOCAL;
  strcpy(address.sun_path, SOCKET_ACTOR);

  if (bind(connection_socket, (sockaddr*)(&address), sizeof(address)) != 0) {
    std::perror("[scenario runner] unable to bind connection socket");
    exit(1);
  }
  // Start queueing incoming connections otherwise there might be a race
  // condition where vsg_init is called before the server side socket is
  // listening.
  if (listen(connection_socket, 1) != 0) {
    std::perror("[scenario runner] unable to listen on connection socket");
    exit(1);
  }
  printf("[scenario runner]]Actor is now ready to listen to connections\n");

  int life_pipe[2];
  if (pipe(life_pipe) != 0) {
    std::perror("[scenario runner] unable to create life_pipe\n");
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
    printf("\t[tansiv] Waiting connections\n");
    int client_socket = accept(connection_socket, (sockaddr*)(&vm_address), &len);
    if (client_socket < 0) {
      std::perror("[tansiv] unable to accept connection on socket");
      exit(1);
    }
    printf("\t[tansiv] Client connection accepted\n");
    // run it
    (*the_scenario)(client_socket);

    // Wait for the parent to (abnormally) exit or (normally) terminate us by SIGQUIT
    char dummy;
    read(life_pipe[0], &dummy, 1);
    printf("[tansiv] Exiting\n");
    exit(0);
  } else if (pid > 0) {
    // Parent: close now unused fds
    close(connection_socket);
    close(life_pipe[0]);
    // sets the attributes
    // printf("[client] I'm your father (my child=%d)\n", pid);
    this->child_pid    = pid;
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

/*
 * Build and send an goto deadline message
 */
static int fb_send_goto_deadline(int socket)
{

  flatbuffers::FlatBufferBuilder builder;
  auto time          = tansiv::Time(0, 200);
  auto goto_deadline = tansiv::CreateGotoDeadline(builder, &time);
  auto msg           = tansiv::CreateFromTansivMsg(builder, tansiv::FromTansiv_GotoDeadline, goto_deadline.Union());
  builder.FinishSizePrefixed(msg);

  // TODO(msimonin): reliable send
  return send(socket, builder.GetBufferPointer(), builder.GetSize(), 0);
}

/*
 * Build and send an end deadline message
 */
static int fb_send_end_simulation(int socket)
{

  flatbuffers::FlatBufferBuilder builder;
  auto end_simulation = tansiv::CreateEndSimulation(builder);
  auto msg            = tansiv::CreateFromTansivMsg(builder, tansiv::FromTansiv_EndSimulation, end_simulation.Union());
  builder.FinishSizePrefixed(msg);

  // TODO(msimonin): reliable send
  return send(socket, builder.GetBufferPointer(), builder.GetSize(), 0);
}

/*
 * Build and serialize a deliver message
 */
static int fb_send_deliver(int socket)
{

  flatbuffers::FlatBufferBuilder builder;

  std::string data    = MESSAGE;
  auto packet_meta    = tansiv::PacketMeta(inet_addr(SRC), inet_addr(DEST));
  auto payload_offset = builder.CreateVector<uint8_t>((uint8_t*) data.c_str(), data.length());
  auto deliver_packet = tansiv::CreateDeliverPacket(builder, &packet_meta, payload_offset);
  auto msg            = tansiv::CreateFromTansivMsg(builder, tansiv::FromTansiv_DeliverPacket, deliver_packet.Union());
  builder.FinishSizePrefixed(msg);

  // TODO(msimonin): reliable sends
  send(socket, builder.GetBufferPointer(), builder.GetSize(), 0);


  return 0;
}

static void fb_init_sequence(int client_socket)
{
  // send go to deadline packet
  fb_send_goto_deadline(client_socket);
}

static void fb_end_sequence(int client_socket)
{
  fb_send_end_simulation(client_socket);
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
  fb_init_sequence(client_socket);
  fb_end_sequence(client_socket);
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
  fb_init_sequence(client_socket);
  uint8_t buffer[128];

  const tansiv::ToTansivMsg* msg;
  // AtDeadline msgs can arrive before getting the first SendPacket
  // https://gitlab.inria.fr/tansiv/tansiv/-/issues/20
  // so we loop until we get somethiing different than AtDeadline
  // and this message must be a SendPacket
  do {
    int ret = fb_recv(client_socket, buffer);
    msg     = flatbuffers::GetRoot<tansiv::ToTansivMsg>(buffer);
    fb_send_goto_deadline(client_socket);
  } while (msg->content_type() == tansiv::ToTansiv::ToTansiv_AtDeadline);
  REQUIRE(msg->content_type() == tansiv::ToTansiv::ToTansiv_SendPacket);
  auto send_packet = msg->content_as_SendPacket();

  in_addr_t src_expected = inet_addr(SRC);
  REQUIRE(src_expected == send_packet->metadata()->src());

  in_addr_t dst_expected = inet_addr(DEST);
  REQUIRE(dst_expected == send_packet->metadata()->dst());

  REQUIRE(0 == ret);
  std::string expected = MESSAGE;
  std::string actual   = std::string((char *)send_packet->payload()->data());
  REQUIRE(expected == actual);

  fb_end_sequence(client_socket);
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
  fb_init_sequence(client_socket);

  fb_deliver(client_socket);

  printf("Leaving deliver_one scenario\n");

  fb_end_sequence(client_socket);
};

/*
 * Simple fb scenario
 *
 * The actor sends the init_sequence:
 *  - a GoToDeadline message
 * Then it sends the end sequence:
 *  - an EndSimulation message
 *
 */
void fb_simple(int client_socket)
{

  printf("\t[tansiv] Entering fb simple scenario\n");
  fb_init_sequence(client_socket);
  fb_end_sequence(client_socket);
  printf("\t[tansiv] Leaving fb simple scenario\n");
}

/*
 * Deliver fb scenario
 *
 * The actot sends a DeliverMessage
 *
 */
void fb_deliver(int client_socket)
{

  printf("\t[tansiv] Entering fb simple scenario\n");
  fb_init_sequence(client_socket);

  fb_send_deliver(client_socket);

  fb_end_sequence(client_socket);
  printf("\t[tansiv] Leaving fb simple scenario\n");
}
