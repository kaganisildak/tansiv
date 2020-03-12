#include <iostream>
#include <cppunit/TestFixture.h>
#include <cppunit/ui/text/TextTestRunner.h>
#include <cppunit/extensions/HelperMacros.h>
#include <cppunit/extensions/TestFactoryRegistry.h>
#include <cppunit/TestResult.h>
#include <cppunit/TestResultCollector.h>
#include <cppunit/TestRunner.h>
#include <cppunit/BriefTestProgressListener.h>
#include <cppunit/CompilerOutputter.h>
#include <cppunit/XmlOutputter.h>

extern "C"
{
#include "vsg.h"
}

// socket pairs
#include <sys/types.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>

using namespace CppUnit;
using namespace std;
//-----------------------------------------------------------------------------

class TestTansiv : public CppUnit::TestFixture
{
  CPPUNIT_TEST_SUITE(TestTansiv);
  CPPUNIT_TEST(testVsgSendAndReceive);
  CPPUNIT_TEST(testVsgDeliverSendAndReceive);
  CPPUNIT_TEST_SUITE_END();

public:
  void setUp(void);
  void tearDown(void);

protected:
  void testVsgSendAndReceive(void);
  void testVsgDeliverSendAndReceive(void);

private:
  int vm_socket;
  int coord_socket;
};

//-----------------------------------------------------------------------------

void TestTansiv::testVsgSendAndReceive(void)
{
  /*
   * Sending part: vm -> coordinator
   */
  std::string send_data = "send_test";
  vsg_dest dest = {inet_addr("1.2.3.4"), 1234};
  struct vsg_time message_time = {42, 42};
  int ret_vsg = vsg_send_send(vm_socket, message_time, dest, send_data.c_str(), send_data.length());

  /*
   * Receiving part: coord -> vm
   */

  // First we get the order
  uint32_t order;
  vsg_recv_order(coord_socket, &order);
  CPPUNIT_ASSERT_EQUAL((uint32_t)vsg_msg_to_actor_type::VSG_SEND_PACKET, order);

  // Then get the actual messages
  struct vsg_send_packet packet = {0, 0};
  // we first get the message information (time + dest + payload size)
  recv(coord_socket, &packet, sizeof(packet), MSG_WAITALL);
  CPPUNIT_ASSERT_EQUAL((uint64_t)42, packet.send_time.seconds);
  CPPUNIT_ASSERT_EQUAL((uint64_t)42, packet.send_time.useconds);
  CPPUNIT_ASSERT_EQUAL((uint32_t)(send_data.length()), packet.packet.size);

  // then we get the message itself and we split
  //   - the destination address (first part) that is only useful for setting up the communication in SimGrid
  //   - and the data transfer, that correspond to the data actually send through the (simulated) network
  // (nb: we use vm_name.length() to determine the size of the destination address because we assume all the vm id
  // to have the same size)

  // recv the payload
  uint32_t recv_size = packet.packet.size;
  // +1 because of, hum, string...
  char recv_data[recv_size + 1];
  uint32_t s = recv(coord_socket, recv_data, recv_size, MSG_WAITALL);
  // yeah string...
  recv_data[recv_size] = '\0';
  std::string actual_data = std::string(recv_data);
  CPPUNIT_ASSERT_EQUAL(send_data, actual_data);
}

void TestTansiv::testVsgDeliverSendAndReceive(void)
{
  /*
   * Sending part: coordinator -> vm
   */
  std::string data = "deliver_test";
  struct vsg_dest dest = {inet_addr("1.2.3.4"), 1234};
  vsg_deliver_send(coord_socket, dest, data.c_str(), data.length());

  /*
   * Receiving part: vm -> coordinator
   */
  uint32_t order;
  vsg_recv_order(vm_socket, &order);
  CPPUNIT_ASSERT_EQUAL((uint32_t)vsg_msg_from_actor_type::VSG_DELIVER_PACKET, order);

  vsg_packet packet = {0};
  vsg_deliver_recv_1(vm_socket, &packet);

  // +1, hum, because of string ?
  char message[packet.size + 1];
  struct in_addr src = {0};
  vsg_deliver_recv_2(vm_socket, message, packet.size);
  // yeah string...
  message[packet.size] = '\0';
  std::string actual_data = std::string(message);
  CPPUNIT_ASSERT_EQUAL(data, actual_data);
}

void TestTansiv::setUp(void)
{
  int sv[2];
  socketpair(AF_UNIX, SOCK_STREAM, 1, sv);
  vm_socket = sv[0];
  CPPUNIT_ASSERT(vm_socket > 0);
  coord_socket = sv[1];
  CPPUNIT_ASSERT(coord_socket > 0);
}

void TestTansiv::tearDown(void)
{
  printf("tearDown\n");
}

//-----------------------------------------------------------------------------

CPPUNIT_TEST_SUITE_REGISTRATION(TestTansiv);

int main(int argc, char *argv[])
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