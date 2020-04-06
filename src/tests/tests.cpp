#include <cppunit/BriefTestProgressListener.h>
#include <cppunit/CompilerOutputter.h>
#include <cppunit/Exception.h>
#include <cppunit/TestFixture.h>
#include <cppunit/TestResult.h>
#include <cppunit/TestResultCollector.h>
#include <cppunit/TestRunner.h>
#include <cppunit/XmlOutputter.h>
#include <cppunit/extensions/HelperMacros.h>
#include <cppunit/extensions/TestFactoryRegistry.h>
#include <cppunit/ui/text/TextTestRunner.h>
#include <errno.h>
#include <iostream>

#include <atomic>
#include <signal.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <sys/wait.h>
#include <unistd.h>

extern "C" {
#include <fake_vm.h>
#include <vsg.h>
}

/* The socket to use for all the tests. */
#define SOCKET_ACTOR "titi"
/* The message to send for send/deliver tests. */
#define MESSAGE "plop"
/* The destination to use for send tests. */
#define DEST "127.0.0.1"

using namespace CppUnit;
using namespace std;

typedef void scenario(int);

/* For use in deliver_one test. */
std::atomic<bool> message_delivered(false);

pid_t simple_actor(scenario f)
{
  // I don't care about the status...
  scenario* the_scenario = (scenario*)f;
  remove(SOCKET_ACTOR);
  printf("\n---\nCreating Simple Actor\n");
  int connection_socket = socket(AF_LOCAL, SOCK_STREAM, 0);

  struct sockaddr_un address;
  address.sun_family = AF_LOCAL;
  strcpy(address.sun_path, SOCKET_ACTOR);

  if (bind(connection_socket, (sockaddr*)(&address), sizeof(address)) != 0) {
    std::perror("unable to bind connection socket");
  }

  if (listen(connection_socket, 1) != 0) {
    std::perror("unable to listen on connection socket");
  }
  printf("Actor is now ready to listen to connections\n");
  // we can fork here
  // this is the parent thread
  // the child process runs the tests ?
  pid_t pid = fork();
  if (pid == 0) {
    // child process: we continue the actor execution
    struct sockaddr_un vm_address = {0};
    unsigned int len              = sizeof(vm_address);
    printf("\tWaiting connections\n");
    int client_socket = accept(connection_socket, (sockaddr*)(&vm_address), &len);
    if (client_socket < 0)
      std::perror("unable to accept connection on socket");
    printf("\tClient connection accepted\n");
    // runit
    try {
      (*the_scenario)(client_socket);
    } catch (CppUnit::Exception e) {
      // TODO(msimonin): can we do better than that ?
      CppUnit::SourceLine line = e.sourceLine();
      printf("Exception in child process:\n line:%d:  %s\n", line.lineNumber(), e.what());
      exit(142);
    }
    // mimic a server
    // our father will kill us anyway when test test is finished
    sleep(3600);
    exit(0);
  } else if (pid > 0) {
    // parent: we continue the execution flow with the test
    return pid;
  } else {
    exit(1);
  }
}

void finalize(pid_t pid)
{
  /* Terminate child. */
  kill(pid, SIGTERM);
  int status;
  waitpid(pid, &status, 0);

  /* Report an error. */
  if (WIFEXITED(status) && WEXITSTATUS(status) > 0) {
    printf("status=%d\n", WEXITSTATUS(status));
    throw CppUnit::Exception();
  }
}

//-----------------------------------------------------------------------------

class TestTansiv : public CppUnit::TestFixture {
  CPPUNIT_TEST_SUITE(TestTansiv);
  CPPUNIT_TEST(testVsgStart);
  CPPUNIT_TEST(testVsgSend);
  CPPUNIT_TEST(testVsgSendEnsureRaise);
  CPPUNIT_TEST(testVsgDeliver);
  CPPUNIT_TEST_SUITE_END();

public:
  void setUp(void);
  void tearDown(void);

protected:
  void testVsgStart(void);
  void testVsgSend(void);
  void testVsgSendEnsureRaise(void);
  void testVsgDeliver(void);

private:
  /* hold the context created bu vsg_init. */
  vsg_context* context;
};

void recv_cb(const struct vsg_context* context, uint32_t msglen, const uint8_t* msg)
{
  printf("callback called\n");
};

void TestTansiv::setUp(void) {}

void TestTansiv::tearDown(void) {}

void init_sequence(int client_socket)
{
  vsg_msg_in_type msg = vsg_msg_in_type::GoToDeadline;
  int ret             = send(client_socket, &msg, sizeof(uint32_t), 0);
  vsg_time t          = {0, 200};
  send(client_socket, &t, sizeof(vsg_time), 0);
}

void end_sequence(int client_socket)
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
};

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
  CPPUNIT_ASSERT_EQUAL(vsg_msg_out_type::SendPacket, msg_type);

  // second, check the send time and size
  vsg_send_packet send_packet = {0};
  recv(client_socket, &send_packet, sizeof(vsg_send_packet), MSG_WAITALL);

  // test the received address
  in_addr_t dest_expected = inet_addr(DEST);
  CPPUNIT_ASSERT_EQUAL(dest_expected, send_packet.dest);

  // finally get the payload
  uint8_t buf[send_packet.packet.size];
  recv(client_socket, buf, send_packet.packet.size, MSG_WAITALL);
  std::string expected = MESSAGE;
  std::string actual   = std::string((char*)buf);
  CPPUNIT_ASSERT_EQUAL_MESSAGE("payload received by the actor differs from what has been sent by the application",
                               expected, actual);

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

  uint32_t deliver_flag                    = vsg_msg_in_type::DeliverPacket;
  std::string data                         = MESSAGE;
  vsg_packet packet                        = {.size = data.length()};
  struct vsg_deliver_packet deliver_packet = {.packet = packet};
  vsg_deliver_send(client_socket, deliver_packet, data.c_str());
  printf("Leaving deliver_one scenario\n");

  end_sequence(client_socket);
};

void TestTansiv::testVsgStart(void)
{
  pid_t pid = simple_actor(simple);

  int argc                 = 4;
  const char* const argv[] = {"-a", SOCKET_ACTOR, "-t", "1970-01-01T00:00:00"};
  vsg_context* context     = vsg_init(argc, argv, NULL, recv_cb);

  CPPUNIT_ASSERT(context != NULL);
  int ret = vsg_start(context);
  CPPUNIT_ASSERT_EQUAL(0, ret);
  int status;

  vsg_stop(context);
  vsg_cleanup(context);

  finalize(pid);
}

void TestTansiv::testVsgSend(void)
{
  pid_t pid = simple_actor(recv_one);

  int argc                 = 4;
  const char* const argv[] = {"-a", SOCKET_ACTOR, "-t", "1970-01-01T00:00:00"};
  vsg_context* context     = vsg_init(argc, argv, NULL, recv_cb);
  int ret                  = vsg_start(context);
  std::string msg          = MESSAGE;
  in_addr_t dest           = inet_addr(DEST);
  vsg_send(context, dest, msg.length() + 1, (uint8_t*)msg.c_str());

  vsg_stop(context);
  vsg_cleanup(context);

  finalize(pid);
}

/*
 * Not a vsg test but
 * We test our testing procedure: we ensure that the child process is raising an
 * error in case asertion violation. We force the send test to fail.
 */
void TestTansiv::testVsgSendEnsureRaise(void)
{
  pid_t pid = simple_actor(recv_one);

  int argc                 = 4;
  const char* const argv[] = {"-a", SOCKET_ACTOR, "-t", "1970-01-01T00:00:00"};
  vsg_context* context     = vsg_init(argc, argv, NULL, recv_cb);
  int ret                  = vsg_start(context);
  in_addr_t dest           = inet_addr(DEST);
  /* inject an error here msg != MESSAGE*/
  std::string msg = "plop1";
  vsg_send(context, dest, msg.length() + 1, (uint8_t*)msg.c_str());

  vsg_stop(context);
  vsg_cleanup(context);
  bool thrown = false;
  try {
    finalize(pid);
  } catch (CppUnit::Exception e) {
    thrown = true;
  }
  CPPUNIT_ASSERT_MESSAGE("The must throw an exception", thrown);
}

void recv_cb_atomic(const struct vsg_context* context, uint32_t msglen, const uint8_t* msg)
{
  message_delivered = true;
};

void TestTansiv::testVsgDeliver(void)
{

  pid_t pid = simple_actor(deliver_one);

  int argc                 = 4;
  const char* const argv[] = {"-a", SOCKET_ACTOR, "-t", "1970-01-01T00:00:00"};
  vsg_context* context     = vsg_init(argc, argv, NULL, recv_cb_atomic);
  int ret                  = vsg_start(context);

  // loop until our atomic is set to true
  // this shouldn't take long ...
  for (int i = 0; i < 3; i++) {
    if (message_delivered.load())
      break;
    sleep(1);
  }
  CPPUNIT_ASSERT_MESSAGE("Deliver Callback hasn't been received", message_delivered.load());

  vsg_stop(context);
  vsg_cleanup(context);

  finalize(pid);
}

CPPUNIT_TEST_SUITE_REGISTRATION(TestTansiv);

int main(int argc, char* argv[])
{
  // informs test-listener about testresults
  CPPUNIT_NS::TestResult testresult;

  // register listener for collecting the test-results
  CPPUNIT_NS::TestResultCollector collectedresults;
  testresult.addListener(&collectedresults);

  // register listener for per-test progress output
  CPPUNIT_NS::BriefTestProgressListener progress;
  testresult.addListener(&progress);

  // insert test-suite at test-runner by registry
  CPPUNIT_NS::TestRunner testrunner;
  testrunner.addTest(CPPUNIT_NS::TestFactoryRegistry::getRegistry().makeTest());
  testrunner.run(testresult);

  // output results in compiler-format
  CPPUNIT_NS::CompilerOutputter compileroutputter(&collectedresults, std::cerr);
  compileroutputter.write();

  // Output XML for Jenkins CPPunit plugin
  ofstream xmlFileOut("tests.xml");
  XmlOutputter xmlOut(&collectedresults, xmlFileOut);
  xmlOut.write();

  // return 0 if tests were successful
  return collectedresults.wasSuccessful() ? 0 : 1;
}