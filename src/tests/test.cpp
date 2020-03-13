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

class TestUtils : public CppUnit::TestFixture
{
  CPPUNIT_TEST_SUITE(TestUtils);
  CPPUNIT_TEST(testVsgTimeEq);
  CPPUNIT_TEST_SUITE_END();

protected:
  void testVsgTimeEq(void);
};

void TestUtils::testVsgTimeEq(void)
{
  vsg_time time1 = {0, 0};
  vsg_time time2 = {0, 0};
  vsg_time time3 = {42, 42};
  vsg_time time4 = {42, 42};
  double g = 1e6 + 42;
  vsg_time time5 = {41, g};
  CPPUNIT_ASSERT(vsg_time_eq(time1, time2));
  CPPUNIT_ASSERT(!vsg_time_eq(time1, time3));
  CPPUNIT_ASSERT(vsg_time_eq(time3, time4));
  CPPUNIT_ASSERT(vsg_time_eq(time3, time5));
}

//-----------------------------------------------------------------------------

void TestTansiv::testVsgSendAndReceive(void)
{
  /*
   * Sending part: vm -> coordinator
   */
  std::string send_data = "send_test";
  vsg_addr dest = {inet_addr("2.2.3.4"), 1234};
  vsg_addr src = {inet_addr("127.0.0.1"), 4321};
  struct vsg_time message_time = {42, 44};
  struct vsg_packet packet = {
      .size = send_data.length(),
      .dest = dest,
      .src = src};
  struct vsg_send_packet send_packet = {
      .send_time = message_time,
      .packet = packet};

  int ret_vsg = vsg_send_send(vm_socket, send_packet, send_data.c_str());

  /*
   * Receiving part: coord -> vm
   */

  // First we get the order
  uint32_t order;
  vsg_recv_order(coord_socket, &order);
  CPPUNIT_ASSERT_EQUAL((uint32_t)vsg_msg_to_actor_type::VSG_SEND_PACKET, order);

  // Then get the actual messages
  struct vsg_send_packet recv_packet = {0};
  // we first get the message information (time + dest + payload size)
  recv(coord_socket, &recv_packet, sizeof(recv_packet), MSG_WAITALL);
  CPPUNIT_ASSERT_EQUAL((uint64_t)42, recv_packet.send_time.seconds);
  CPPUNIT_ASSERT_EQUAL((uint64_t)44, recv_packet.send_time.useconds);
  CPPUNIT_ASSERT_EQUAL((uint32_t)(send_data.length()), recv_packet.packet.size);
  CPPUNIT_ASSERT_EQUAL(dest.port, recv_packet.packet.dest.port);
  CPPUNIT_ASSERT_EQUAL(dest.addr, recv_packet.packet.dest.addr);
  CPPUNIT_ASSERT_EQUAL(src.port, recv_packet.packet.src.port);
  CPPUNIT_ASSERT_EQUAL(src.addr, recv_packet.packet.src.addr);

  // then we get the message itself and we split
  //   - the destination address (first part) that is only useful for setting up the communication in SimGrid
  //   - and the data transfer, that correspond to the data actually send through the (simulated) network
  // (nb: we use vm_name.length() to determine the size of the destination address because we assume all the vm id
  // to have the same size)

  // recv the payload
  uint32_t recv_size = recv_packet.packet.size;
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
  std::string deliver_data = "deliver_test";
  vsg_addr dest = {inet_addr("2.2.3.4"), 1235};
  vsg_addr src = {inet_addr("127.0.0.1"), 4321};
  struct vsg_packet packet = {
      .size = deliver_data.length(),
      .dest = dest,
      .src = src};
  struct vsg_deliver_packet deliver_packet = {
      packet = packet};
  vsg_deliver_send(coord_socket, deliver_packet, deliver_data.c_str());

  /*
   * Receiving part: vm -> coordinator
   */
  uint32_t order;
  vsg_recv_order(vm_socket, &order);
  CPPUNIT_ASSERT_EQUAL((uint32_t)vsg_msg_from_actor_type::VSG_DELIVER_PACKET, order);

  vsg_deliver_packet recv_packet = {0};
  vsg_deliver_recv_1(vm_socket, &recv_packet);
  CPPUNIT_ASSERT_EQUAL((uint32_t)(deliver_data.length()), recv_packet.packet.size);
  CPPUNIT_ASSERT_EQUAL(dest.port, recv_packet.packet.dest.port);
  CPPUNIT_ASSERT_EQUAL(dest.addr, recv_packet.packet.dest.addr);
  CPPUNIT_ASSERT_EQUAL(src.port, recv_packet.packet.src.port);
  CPPUNIT_ASSERT_EQUAL(src.addr, recv_packet.packet.src.addr);

  // +1, hum, because of string ?
  char message[recv_packet.packet.size + 1];
  vsg_deliver_recv_2(vm_socket, message, recv_packet.packet.size);
  // yeah string...
  message[recv_packet.packet.size] = '\0';
  std::string actual_data = std::string(message);
  CPPUNIT_ASSERT_EQUAL(deliver_data, actual_data);
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
CPPUNIT_TEST_SUITE_REGISTRATION(TestUtils);

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