#define CATCH_CONFIG_MAIN // This tells Catch to provide a main() - only do this in one cpp file

#include "catch.hpp"
#include <atomic>

#include "scenario.hpp"

void recv_cb(uintptr_t arg)
{
  // Try not to deadlock with libc's stdout
  const char hey[] = "callback called\n";
  write(STDOUT_FILENO, hey, sizeof(hey));
};

void deadline_cb(uintptr_t arg, struct timespec deadline)
{
  // Try not to deadlock with libc's stdout
  const char hey[] = "deadline set\n";
  write(STDOUT_FILENO, hey, sizeof(hey));
}

void recv_cb_atomic(uintptr_t arg)
{
  std::atomic<bool>* message_delivered = (std::atomic<bool>*)arg;
  *message_delivered                   = true;
};

/**
 * Util fonction to get the port out of an incoming message.
 */
in_port_t recv_pg(const struct vsg_context* context)
{
  uint8_t payload[sizeof(MESSAGE) + sizeof(in_port_t)];
  uint32_t payload_len = sizeof(payload);
  uint32_t src, dst;
  int ret = vsg_recv(context, &src, &dst, &payload_len, payload);
  if (ret) {
    // Return 0 as an error port number
    return 0;
  }

  // un-piggyback
  in_port_t recv_port;
  uint8_t* recv_payload;
  vsg_upg_port((void*)payload, payload_len, &recv_port, &recv_payload);
  return recv_port;
};

TEST_CASE("initialize the vsg client", "[vsg]")
{
  ScenarioRunner s = ScenarioRunner(simple);
  SECTION("simple")
  {

    int argc                 = 6;
    const char* const argv[] = {"-a", SOCKET_ACTOR, "-n", SRC, "-t", "1970-01-01T00:00:00"};

    vsg_context* context = vsg_init(argc, argv, NULL, recv_cb, 0, deadline_cb, 0);
    REQUIRE(context != NULL);

    int ret = vsg_start(context, NULL);
    REQUIRE(ret == 0);

    vsg_stop(context);
    vsg_cleanup(context);
  }
}

TEST_CASE("VSG receive one message", "[vsg]")
{
  ScenarioRunner s = ScenarioRunner(recv_one);
  SECTION("simple")
  {

    int argc                 = 6;
    const char* const argv[] = {"-a", SOCKET_ACTOR, "-n", SRC, "-t", "1970-01-01T00:00:00"};
    vsg_context* context     = vsg_init(argc, argv, NULL, recv_cb, 0, deadline_cb, 0);
    REQUIRE(context != NULL);

    int ret = vsg_start(context, NULL);
    REQUIRE(ret == 0);

    std::string msg = MESSAGE;
    in_addr_t dst   = inet_addr(DEST);
    vsg_send(context, dst, msg.length() + 1, (uint8_t*)msg.c_str());

    vsg_stop(context);
    vsg_cleanup(context);
  }
}

TEST_CASE("VSG deliver one message", "[vsg]")
{
  ScenarioRunner s = ScenarioRunner(deliver_one);
  SECTION("_")
  {

    int argc                 = 6;
    const char* const argv[] = {"-a", SOCKET_ACTOR, "-n", SRC, "-t", "1970-01-01T00:00:00"};
    std::atomic<bool> message_delivered(false);
    vsg_context* context = vsg_init(6, argv, NULL, recv_cb_atomic, (uintptr_t)&message_delivered, deadline_cb, 0);
    REQUIRE(context != NULL);

    int ret = vsg_start(context, NULL);
    REQUIRE(ret == 0);

    // loop until our atomic is set to true
    // this shouldn't take long ...
    for (int i = 0; i < 3; i++) {
      if (message_delivered.load())
        break;
      sleep(1);
    }
    REQUIRE(message_delivered.load());

    uint32_t msg_len = strlen(MESSAGE) + 1;
    uint32_t src, dst;
    uint8_t* buffer = (uint8_t*)malloc(strlen(MESSAGE) + 1);
    // Test the received message
    vsg_recv(context, &src, &dst, &msg_len, buffer);

    // test the received message
    // -- size read
    REQUIRE((size_t)msg_len == strlen(MESSAGE) + 1);
    REQUIRE(inet_addr(SRC) == src);
    REQUIRE(inet_addr(DEST) == dst);

    // -- payload
    std::string actual   = std::string((char*)buffer);
    std::string expected = std::string(MESSAGE);
    REQUIRE(expected == actual);

    vsg_stop(context);
    vsg_cleanup(context);
  }
}

TEST_CASE("VSG send piggyback port", "[vsg]")
{
  ScenarioRunner s = ScenarioRunner(send_deliver_pg_port);
  SECTION("_")
  {

    int argc                 = 6;
    const char* const argv[] = {"-a", SOCKET_ACTOR, "-n", SRC, "-t", "1970-01-01T00:00:00"};
    vsg_context* context     = vsg_init(argc, argv, NULL, recv_cb, 0, deadline_cb, 0);
    REQUIRE(context != NULL);

    int ret = vsg_start(context, NULL);
    REQUIRE(ret == 0);

    in_port_t port  = 5000;
    std::string msg = MESSAGE;
    in_addr_t dst   = inet_addr(DEST);

    int payload_length = msg.length() + sizeof(in_port_t) + 1; // because of str
    uint8_t payload[payload_length];
    // piggyback
    vsg_pg_port(port, (uint8_t*)msg.c_str(), msg.length() + 1, payload);

    // fire!
    vsg_send(context, dst, payload_length, payload);

    // loop until some message arrives
    // this shouldn't take long ...
    for (int i = 0; i < 3; i++) {
      if (vsg_poll(context) == 0)
        break;
      sleep(1);
    }

    // test the receive port
    REQUIRE(port == recv_pg(context));

    vsg_stop(context);
    vsg_cleanup(context);
  }
}

TEST_CASE("piggyback port", "[novsg]")
{
  in_port_t port     = 5000;
  std::string msg    = MESSAGE;
  int payload_length = msg.length() + sizeof(in_port_t) + 1; // because of str
  uint8_t payload[payload_length];
  // piggyback
  vsg_pg_port(port, (uint8_t*)msg.c_str(), msg.length() + 1, payload);

  // un-piggyback
  in_port_t recv_port;
  uint8_t* recv_payload;
  vsg_upg_port(payload, payload_length, &recv_port, &recv_payload);
  // test the receive port
  REQUIRE(port == recv_port);

  // test the receive message
  std::string actual_msg = std::string((char*)recv_payload);
  REQUIRE(msg == actual_msg);
}
