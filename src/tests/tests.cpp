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


TEST_CASE("initialize the vsg client", "[vsg]")
{
  ScenarioRunner s = ScenarioRunner(simple);
  SECTION("simple")
  {

    const char* const argv[] = {"-a", SOCKET_ACTOR, "-n", SRC, "-w100000000", "-x24", "-t", "1970-01-01T00:00:00"};
    int argc                 = sizeof(argv) / sizeof(argv[0]);

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

    const char* const argv[] = {"-a", SOCKET_ACTOR, "-n", SRC, "-w100000000", "-x24", "-t", "1970-01-01T00:00:00"};
    int argc                 = sizeof(argv) / sizeof(argv[0]);
    vsg_context* context     = vsg_init(argc, argv, NULL, recv_cb, 0, deadline_cb, 0);
    REQUIRE(context != NULL);

    int ret = vsg_start(context, NULL);
    REQUIRE(ret == 0);

    std::string msg = MESSAGE;
    in_addr_t dst   = inet_addr(DEST);
    ret             = vsg_send(context, dst, msg.length() + 1, (uint8_t*)msg.c_str());
    REQUIRE(ret == 0);

    vsg_stop(context);
    vsg_cleanup(context);
  }
}

TEST_CASE("VSG deliver one message with atomic", "[vsg]")
{
  ScenarioRunner s = ScenarioRunner(deliver_one);
  SECTION("_")
  {

    const char* const argv[] = {"-a", SOCKET_ACTOR, "-n", SRC, "-w100000000", "-x24", "-t", "1970-01-01T00:00:00"};
    int argc                 = sizeof(argv) / sizeof(argv[0]);
    std::atomic<bool> message_delivered(false);
    vsg_context* context = vsg_init(argc, argv, NULL, recv_cb_atomic, (uintptr_t)&message_delivered, deadline_cb, 0);
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

    uint32_t msg_len = strlen(MESSAGE);
    uint32_t src, dst;
    uint8_t buffer[msg_len + 1];
    // Test the received message
    ret = vsg_recv(context, &src, &dst, &msg_len, buffer);
    // add the nul termination
    buffer[msg_len] = 0;
    REQUIRE(0 == ret);

    // test the received message
    // -- size read
    REQUIRE((size_t)msg_len == strlen(MESSAGE));
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

TEST_CASE("VSG deliver one message with vsg_poll", "[vsg]")
{
  ScenarioRunner s = ScenarioRunner(deliver_one);
  SECTION("_")
  {

    const char* const argv[] = {"-a", SOCKET_ACTOR, "-n", SRC, "-w100000000", "-x24", "-t", "1970-01-01T00:00:00"};
    int argc                 = sizeof(argv) / sizeof(argv[0]);
    std::atomic<bool> message_delivered(false);
    vsg_context* context = vsg_init(argc, argv, NULL, recv_cb, (uintptr_t)&message_delivered, deadline_cb, 0);
    REQUIRE(context != NULL);

    int ret = vsg_start(context, NULL);
    REQUIRE(ret == 0);

    // loop until some message arrives
    // this shouldn't take long ...
    int i;
    for (i = 0; i < 3; i++) {
      if (vsg_poll(context) == 0)
        break;
      sleep(1);
    }

    uint32_t msg_len = strlen(MESSAGE);
    uint32_t src, dst;
    uint8_t buffer[msg_len + 1];
    // Test the received message
    ret = vsg_recv(context, &src, &dst, &msg_len, buffer);
    // add the nul termination
    buffer[msg_len] = 0;
    REQUIRE(0 == ret);

    // test the received message
    // -- size read
    REQUIRE((size_t)msg_len == strlen(MESSAGE));
    REQUIRE(inet_addr(SRC) == src);
    REQUIRE(inet_addr(DEST) == dst);

    // test the received message
    // -- size read
    REQUIRE((size_t)msg_len == strlen(MESSAGE));

    // -- some metadata
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