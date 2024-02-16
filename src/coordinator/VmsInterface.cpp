#include "VmsInterface.hpp"
#include <algorithm>
#include <limits>
#include <signal.h>
#include <unistd.h>
#include <xbt/log.hpp>

#include "socket.hpp"

// big enough buffer for incoming messages
#define SCRATCH_BUFFER_LEN 2048

//#define LOG_MESSAGES 1

// Enable to get some log about message create/copy/move
// #define LOG_MESSAGES 1

XBT_LOG_NEW_DEFAULT_CATEGORY(vm_interface, "Logging specific to the VmsInterface");

namespace vsg {

bool sortMessages(Message* i, Message* j)
{
  return i->sent_time < j->sent_time;
}

vsg_time simgridToVmTime(double simgrid_time)
{
  struct vsg_time vm_time;

  // when only one VM remains, simgrid_time = DOUBLE_MAX.
  // we use the code below to avoid conversion issue of DOUBLE_MAX to uint64_t.
  if (simgrid_time > std::numeric_limits<uint64_t>::max()) {
    vm_time.seconds  = std::numeric_limits<uint64_t>::max();
    vm_time.nseconds = std::numeric_limits<uint64_t>::max();
  } else {
    // the simgrid time correspond to a double in second, so the number of seconds is the integer part
    vm_time.seconds = (uint64_t)(std::floor(simgrid_time));
    // and the number of usecond is the decimal number scaled accordingly
    vm_time.nseconds = (uint64_t)(std::floor((simgrid_time - std::floor(simgrid_time)) * 1e9));
  }

  return vm_time;
}

double vmToSimgridTime(uint64_t seconds, uint64_t nseconds)
{
  return seconds + nseconds * 1e-9;
}

double vmToSimgridTime(vsg_time vm_time)
{
  return vmToSimgridTime(vm_time.seconds, vm_time.nseconds);
}

VmsInterface::VmsInterface(std::string connection_socket_name, bool stop_at_any_stop)
{
  a_vm_stopped              = false;
  simulate_until_any_stop   = stop_at_any_stop;
  socket_name               = connection_socket_name;
  const char* c_socket_name = socket_name.c_str();

  remove(c_socket_name);
  connection_socket = socket(PF_LOCAL, SOCK_STREAM, 0);
  XBT_INFO("socket created");

  struct sockaddr_un address;
  address.sun_family = AF_LOCAL;
  strcpy(address.sun_path, c_socket_name);

  if (bind(connection_socket, (sockaddr*)(&address), sizeof(address)) != 0) {
    std::perror("unable to bind connection socket");
    end_simulation();
  }
  XBT_VERB("socket binded");

  if (listen(connection_socket, 1) != 0) {
    std::perror("unable to listen on connection socket");
    end_simulation();
  }
  XBT_VERB("listen on socket");

  signal(SIGPIPE, SIG_IGN);
}

VmsInterface::~VmsInterface()
{
  end_simulation(true, false);
}

void VmsInterface::register_vm(std::string host_name, std::string vm_name, std::string file,
                               std::vector<std::string> argv)
{

  vm_deployments[vm_name] = host_name;

  std::vector<char*> command;
  // we inject the socket name as the first parameter
  // we thus have a convention here for the program launched by tansiv
  // they must handle the first parameter as the socket name.
  std::string exec_line = file;
  for (auto const& arg : argv) {
    command.emplace_back(const_cast<char*>(arg.c_str()));
    exec_line += " " + arg;
  }
  command.push_back(nullptr);
  auto it = command.begin();
  command.insert(it + 1, (char*)socket_name.c_str());

  // XBT_INFO("fork and exec of [%s]", exec_line.c_str());

  switch (fork()) {
    case -1:
      std::perror("unable to fork process");
      end_simulation();
      break;
    case 0:
      end_simulation(false, false);

      // inject the socket name as the first parameter
      if (execvp(file.c_str(), command.data()) != 0) {
        std::perror("unable to launch VM");
        end_simulation();
      }
      break;
    default:
      break;
  }
  XBT_VERB("fork done for VM %s", vm_name.c_str());

  struct sockaddr_un vm_address = {0};
  unsigned int len              = sizeof(vm_address);
  int vm_socket                 = accept(connection_socket, (sockaddr*)(&vm_address), &len);
  if (vm_socket < 0)
    std::perror("unable to accept connection on socket");

  vm_sockets[vm_name] = vm_socket;
  XBT_INFO("connection for VM %s established", vm_name.c_str());
}

void VmsInterface::end_simulation(bool must_unlink, bool must_exit)
{
  close(connection_socket);
  for (auto it : vm_sockets) {
    close(it.second);
  }
  XBT_VERB("vm sockets are down");

  if (must_unlink)
    unlink(socket_name.c_str());

  if (must_exit) {
    exit(666);
  }
}

bool VmsInterface::vmActive()
{
  return (!vm_sockets.empty() && !simulate_until_any_stop) || (!a_vm_stopped && simulate_until_any_stop);
}

std::vector<Message*> VmsInterface::goTo(double deadline)
{
  // Beforehand, forget about the VMs that bailed out recently.
  // We hope that the coordinator cleaned the SimGrid side in between
  vm_sockets_trash.clear();

  // TODO(msimonin): reuse that
  flatbuffers::FlatBufferBuilder builder(128);

  // first, we ask all the VMs to go to deadline
  XBT_DEBUG("Sending: go to deadline %f", deadline);
  // FIXME(msimonin)
  struct vsg_time vm_deadline = simgridToVmTime(deadline);

  // goto deadline message
  auto time          = tansiv::Time(vm_deadline.seconds, vm_deadline.nseconds);
  auto goto_deadline = tansiv::CreateGotoDeadline(builder, &time);
  auto msg           = tansiv::CreateFromTansivMsg(builder, tansiv::FromTansiv_GotoDeadline, goto_deadline.Union());
  builder.FinishSizePrefixed(msg);

  for (auto it : vm_sockets) {
    XBT_DEBUG("-- to %s(size=%d)", it.first.c_str(), builder.GetSize());
    vsg_protocol_send(it.second, builder.GetBufferPointer(), builder.GetSize());
  }

  // then, we pick up all the messages send by the VM until they reach the deadline
  std::vector<Message*> messages;
  XBT_INFO("getting the message send by the VMs");

  uint8_t scratch_buffer[SCRATCH_BUFFER_LEN];

  for (auto kv : vm_sockets) {
    std::string vm_name = kv.first;
    int vm_socket       = kv.second;

    // we loop until we get an at_deadline
    bool finished = false;
    while (!finished) {
      if (fb_recv(vm_socket, scratch_buffer, SCRATCH_BUFFER_LEN) < 0) {
        XBT_INFO("can not receive the flags of VM %s. Forget about the socket that seem closed at the system level.",
                 vm_name.c_str());
        close_vm_socket(vm_name);
        break;
      }
      auto msg = flatbuffers::GetRoot<tansiv::ToTansivMsg>(scratch_buffer);
      switch (msg->content_type()) {

        case tansiv::ToTansiv_AtDeadline:
          finished = true;
          break;

        case tansiv::ToTansiv_SendPacket: {
          auto send_packet = msg->content_as_SendPacket();
          // The returned pointer can be nullptr. This happens for instance when
          // the content_type is inconsistent with the actual type of the
          // content yes fbb doesn't prevent this inconsistency (fbb 2.0.0)
          if (send_packet == nullptr) {
            XBT_ERROR("Deserialization error: type of content must be SendPacket");
            break;
          }
          // Our schema use an fbb table (see packets.fbs) Fields on a table can
          // be null however our protocol doesn't allow null fields so we're
          // checking every single field before accepting the message
          auto metadata = send_packet->metadata();
          if (metadata == nullptr) {
            XBT_ERROR("Deserialization error: metadata can't be empty");
            break;
          }
          auto time = send_packet->time();
          if (time == nullptr) {
            XBT_ERROR("Deserialization error: time can't be empty");
            break;
          }
          const flatbuffers::Vector<uint8_t>* payload = send_packet->payload();
          if (payload == nullptr) {
            XBT_ERROR("Deserialization error: payload can't be empty");
            break;
          }
          // build our own internal message structure and add it to the list of flying messages
          auto message = new Message(time->seconds(), time->nseconds(), metadata->src(), metadata->dst(),
                                     flatbuffers::VectorLength<uint8_t>(payload), (uint8_t*)payload->data());
          messages.push_back(message);
          break;
        }
        default:
          XBT_ERROR("Unknown message received from VM %s", vm_name.c_str());
          end_simulation();
          finished = true;
          break;
      }
    }
  }

  // Remove all invalid sockets from our list, but leave a chance to the coordinator to notice about them
  for (auto sock_name : vm_sockets_trash)
    vm_sockets.erase(sock_name);

  XBT_DEBUG("forwarding all the %lu messages to SimGrid", messages.size());
  std::sort(messages.begin(), messages.end(), sortMessages);

  return messages;
}

std::string VmsInterface::getHostOfVm(std::string vm_name)
{
  if (vm_deployments.find(vm_name) == vm_deployments.end()) {
    return "";
  }
  return vm_deployments[vm_name];
}

void VmsInterface::close_vm_socket(std::string vm_name)
{
  int vm_socket = vm_sockets.at(vm_name);
  shutdown(vm_socket, SHUT_RDWR);
  close(vm_socket);
  vm_sockets_trash.push_back(vm_name);
  a_vm_stopped = true;
}
const std::vector<std::string> VmsInterface::get_dead_vm_hosts()
{
  std::vector<std::string> dead_hosts;
  for (std::string vm : vm_sockets_trash) {
    dead_hosts.push_back(getHostOfVm(vm));
  }

  return dead_hosts;
}

void VmsInterface::deliverMessage(Message* m)
{
  if (vm_sockets.find(m->dst) != vm_sockets.end()) {
    int socket = vm_sockets[m->dst];

    flatbuffers::FlatBufferBuilder builder(2048);
    auto packet_meta    = tansiv::PacketMeta(m->src_enc, m->dst_enc);
    auto payload_offset = builder.CreateVector<uint8_t>(m->data, m->size);
    auto deliver_packet = tansiv::CreateDeliverPacket(builder, &packet_meta, payload_offset);
    auto msg =
        tansiv::CreateFromTansivMsg(builder, tansiv::FromTansiv::FromTansiv_DeliverPacket, deliver_packet.Union());
    builder.FinishSizePrefixed(msg);
    vsg_protocol_send(socket, builder.GetBufferPointer(), builder.GetSize());

    XBT_VERB("message from vm %s delivered to vm %s size=%u (on the wire size=%d)", m->src.c_str(), m->dst.c_str(),
             m->size, builder.GetSize());
  } else {
    XBT_WARN("message from vm %s was not delivered to vm %s because it already stopped its execution", m->src.c_str(),
             m->dst.c_str());
  }
  delete m;
}

Message::Message(uint64_t seconds, uint64_t nseconds, in_addr_t src_enc, in_addr_t dst_enc, uint32_t size,
                 uint8_t* payload)
    : seconds(seconds), nseconds(nseconds), src_enc(src_enc), dst_enc(dst_enc), size(size)
{
  // -- compute sent time the sent_time
  this->sent_time = vmToSimgridTime(seconds, nseconds);

  // -- then src and dest and make them a std::string
  char src_addr[INET_ADDRSTRLEN];
  char dst_addr[INET_ADDRSTRLEN];
  struct in_addr _src_addr = {src_enc};
  struct in_addr _dst_addr = {dst_enc};
  inet_ntop(AF_INET, &(_src_addr), src_addr, INET_ADDRSTRLEN);
  inet_ntop(AF_INET, &(_dst_addr), dst_addr, INET_ADDRSTRLEN);
  this->src = std::string(src_addr);
  this->dst = std::string(dst_addr);

  // -- finally handle the payload
  this->data = new uint8_t[size];
  memcpy(this->data, payload, size);
#ifdef LOG_MESSAGES
  fprintf(stderr, "Creating new Message@%p: size=%d, data@%p\n", this, this->size, this->data);
#endif
};

Message::Message(const Message& other)
    : Message(other.seconds, other.nseconds, other.src_enc, other.dst_enc, other.size, other.data)
{
#ifdef LOG_MESSAGES
  fprintf(stderr, "Copied Message[%p]: size=%d, data@%p from message[%p]\n", this, this->size, this->data, &other);
#endif
}

Message::Message(Message&& other) : data(nullptr)
{
  // uses the assignement
  *this = std::move(other);
#ifdef LOG_MESSAGES
  fprintf(stderr, "Moved Message[%p]: size=%d, data@%p from Message[%p]\n", this, this->size, this->data, &other);
#endif
}

Message& Message::operator=(Message&& other)
{
  if (this != &other) {
    delete[] this->data;
    this->seconds   = other.seconds;
    this->nseconds  = other.nseconds;
    this->src_enc   = other.src_enc;
    this->dst_enc   = other.dst_enc;
    this->size      = other.size;
    this->sent_time = other.sent_time;
    this->src       = other.src;
    this->dst       = other.dst;
    this->data      = other.data;

    other.data = nullptr;
  }
#ifdef LOG_MESSAGES
  fprintf(stderr, "Moved assigned Message[%p]: size=%d, data@%p from message[%p]\n", this, this->size, this->data, &other);
#endif
  return *this;
}

Message::~Message()
{
  if (this->data != nullptr) {
    delete[] this->data;
#ifdef LOG_MESSAGES
    fprintf(stderr, "Destructing message[%p]: size=%d, data@%p\n", this, this->size, this->data);
#endif
  }
}
} // namespace vsg
