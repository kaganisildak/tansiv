#include <cppunit/BriefTestProgressListener.h>
#include <cppunit/CompilerOutputter.h>
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

extern "C" {
#include <fake_vm.h>
}

#include <pthread.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>

#define SOCKET_ACTOR "titi"

typedef void scenario(int);

void *simple_actor(void *f) {
  // I don't care about the status...
  scenario *the_scenario = (scenario *)f;
  remove(SOCKET_ACTOR);
  printf("\n---\nCreating Simple Actor\n");
  int connection_socket = socket(AF_LOCAL, SOCK_STREAM, 0);

  struct sockaddr_un address;
  address.sun_family = AF_LOCAL;
  strcpy(address.sun_path, SOCKET_ACTOR);

  if (bind(connection_socket, (sockaddr *)(&address), sizeof(address)) != 0) {
    std::perror("unable to bind connection socket");
  }

  if (listen(connection_socket, 1) != 0) {
    std::perror("unable to listen on connection socket");
  }
  struct sockaddr_un vm_address = {0};
  unsigned int len = sizeof(vm_address);
  printf("\tWaiting connections\n");
  int client_socket =
      accept(connection_socket, (sockaddr *)(&vm_address), &len);
  if (client_socket < 0)
    std::perror("unable to accept connection on socket");
  printf("\tClient connection accepted\n");

  // runit
  (*the_scenario)(client_socket);

  pthread_exit(NULL);
}

void init_actor(pthread_t *thread, scenario s) {
  pthread_create(thread, NULL, simple_actor, (void *)s);
}

using namespace CppUnit;
using namespace std;
//-----------------------------------------------------------------------------

class TestTansiv : public CppUnit::TestFixture {
  CPPUNIT_TEST_SUITE(TestTansiv);
  CPPUNIT_TEST(testVsgStart);
  CPPUNIT_TEST(testVsgSend);
  CPPUNIT_TEST_SUITE_END();

public:
  void setUp(void);
  void tearDown(void);

protected:
  void testVsgStart(void);
  void testVsgSend(void);

private:
  /*hold the context created bu vsg_init.*/
  vsg_context *context;
};

void recv_cb(const struct vsg_context *context, uint32_t msglen,
             const uint8_t *msg) {
  printf("callback called\n");
};

void TestTansiv::setUp(void) {}

void TestTansiv::tearDown(void) {
  vsg_stop(context);
  vsg_cleanup(context);
}

void init_sequence(int client_socket) {
  vsg_msg_in_type msg = vsg_msg_in_type::GoToDeadline;
  send(client_socket, &msg, sizeof(uint32_t), 0);
  vsg_time t = {0, 200};
  send(client_socket, &t, sizeof(vsg_time), 0);
  msg = vsg_msg_in_type::EndSimulation;
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
void simple(int client_socket) {
  printf("Entering simple scenario\n");
  init_sequence(client_socket);
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
void recv_one(int client_socket) {
  printf("Entering recv_one scenario\n");
  init_sequence(client_socket);
  // first, check the type of message
  vsg_msg_out_type msg_type;
  recv(client_socket, &msg_type, sizeof(vsg_msg_out_type), MSG_WAITALL);
  CPPUNIT_ASSERT_EQUAL(vsg_msg_out_type::SendPacket, msg_type);
  // second, check the send time and size
  vsg_send_packet send_packet;
  recv(client_socket, &send_packet, sizeof(vsg_send_packet), MSG_WAITALL);
  // TODO(msimonin): can we test something here ?
  // CPPUNIT_ASSERT_EQUAL((uint64_t)0, time.seconds);
  // CPPUNIT_ASSERT_LESSEQUAL((uint64_t)200, time.useconds);

  // finally get the payload
  uint8_t buf[send_packet.packet.size];
  recv(client_socket, buf, send_packet.packet.size, MSG_WAITALL);
  std::string expected = "plop";
  std::string actual = std::string((char *)buf);
  CPPUNIT_ASSERT_EQUAL(expected, actual);
  printf("Leaving recv_one scenario\n");
};

void TestTansiv::testVsgStart(void) {
  /*holt the actor thread where the scenario are run.*/
  pthread_t actor_thread;
  init_actor(&actor_thread, simple);

  int argc = 4;
  const char *const argv[] = {"-a", SOCKET_ACTOR, "-t", "1970-01-01T00:00:00"};
  vsg_context *context = vsg_init(argc, argv, NULL, recv_cb);
  printf("Starting the client\n");
  CPPUNIT_ASSERT(context != NULL);
  int ret = vsg_start(context);
  CPPUNIT_ASSERT_EQUAL(0, ret);

  pthread_join(actor_thread, NULL);
}

void TestTansiv::testVsgSend(void) {
  /*holt the actor thread where the scenario are run.*/
  pthread_t actor_thread;
  init_actor(&actor_thread, recv_one);

  int argc = 4;
  const char *const argv[] = {"-a", SOCKET_ACTOR, "-t", "1970-01-01T00:00:00"};
  vsg_context *context = vsg_init(argc, argv, NULL, recv_cb);
  int ret = vsg_start(context);
  std::string msg = "plop";
  vsg_send(context, msg.length() + 1, (uint8_t *)msg.c_str());

  pthread_join(actor_thread, NULL);
}

CPPUNIT_TEST_SUITE_REGISTRATION(TestTansiv);

int main(int argc, char *argv[]) {
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