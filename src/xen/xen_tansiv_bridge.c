/*
 * Interface between TANSIV-client and Xen by using LibVMI
 */

#include <assert.h>
#include <errno.h>
#include <inttypes.h>
#include <poll.h>
#include <pthread.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>

#include <libvmi/events.h>
#include <libvmi/libvmi.h>

#include <arpa/inet.h>
#include <netinet/if_ether.h>
#include <netinet/in.h>  //enums IPPROTO ...
#include <netinet/ip.h>  // struct ip, iphdr ...
#include <netinet/udp.h> // struct udphdr...

#include <tansiv-client.h>
#include <tansiv-timer-xen.h>

#define XC_WANT_COMPAT_MAP_FOREIGN_API // For xc_map_foreign_range
#include <xenctrl.h>

#define MTU 1500

struct vsg_context* context;
int fd;
struct pollfd pfd;

void tantap_vsg_receive_cb(uintptr_t arg __attribute__((unused)))
{
  uint8_t buf[MTU];
  uint32_t src, dst;
  uint32_t msg_len = MTU;
  while (vsg_poll(context) == 0) {
    vsg_recv(context, &src, &dst, &msg_len, buf);
    // Send the packet to the kernel module
    write(fd, buf, msg_len);
  }
}

void dummy_vsg_deadline_cb(uintptr_t arg __attribute__((unused)), struct timespec deadline __attribute__((unused))) {}

event_response_t dummy_cb(vmi_instance_t vmi __attribute__((unused)), vmi_event_t* event __attribute__((unused)))
{
  return VMI_EVENT_RESPONSE_NONE;
}

void vsg_setup(const char* socket, const char* src, uint64_t num_buffers)
{
  int vsg_argc = 8;
  char num_buffers_c[20];
  sprintf(num_buffers_c, "%ld", num_buffers);
  const char* const vsg_argv[] = {"-a", socket, "-n", src, "-b", num_buffers_c, "-t", "1970-01-01T00:00:00"};

  printf("socket: %s\n", vsg_argv[1]);
  printf("src: %s\n", vsg_argv[3]);
  printf("num_buffers_c: %s\n", vsg_argv[5]);

  /* vsg_init args
   * argc
   * argv
   * next_arg_p
   * recv_callback
   * recv_callback_arg
   * deadline_callback
   * deadline_callback_arg
   */
  context = vsg_init(vsg_argc, vsg_argv, NULL, tantap_vsg_receive_cb, (uintptr_t)0, dummy_vsg_deadline_cb, 0);
  assert(context != NULL);

  int ret = vsg_start(context, NULL);
  assert(ret == 0);
}

static int interrupted = 0;
static void close_handler(int sig)
{
  interrupted = sig;
}

extern uint64_t deadline_handler(const struct vsg_context* context, uint64_t guest_tsc) __attribute__((weak));
extern int get_tansiv_timer_fd(const struct vsg_context* context) __attribute__((weak));
extern int set_tansiv_tsc_page(const struct vsg_context* context, void* memory) __attribute__((weak));

event_response_t tansiv_deadline_callback(vmi_instance_t vmi, vmi_event_t* event)
{
  // TODO: Don't use libvmi for this hypercall
  vmi_event_t resume_event = {0};
  resume_event.version     = VMI_EVENTS_VERSION;
  resume_event.type        = VMI_EVENT_RESUME;
  resume_event.callback    = dummy_cb;

  // printf("TANSIV deadline callback\n");
  uint64_t tsc_deadline = deadline_handler(context, 0);
  // printf("deadline_handler call done\n");

  event->tsc_deadline = tsc_deadline;

  if (vmi_register_event(vmi, &resume_event) == VMI_FAILURE)
    fprintf(stderr, "Resume event failed\n");
  // else
  // printf("Resume event success\n");

  vmi_resume_vm(vmi);

  return VMI_EVENT_RESPONSE_NONE;
}

void packet_dump(uint8_t* buf, int size)
{
  printf("****Packet of size %d****\n", size);
  for (int i = 0; i < size; i++) {
    printf("%02x ", buf[i]);
    if ((i + 1) % 16 == 0)
      printf("\n");
  }
  printf("\n*******************\n\n");
}

void* read_packets(void* unused __attribute__((unused)))
{
  int ret;
  uint8_t buf[MTU];
  struct iphdr* iphdr;
  in_addr_t dest;

  while (true) {
    poll(&pfd, 1, -1);
    if (pfd.revents & POLLIN) {
      ret = read(fd, buf, MTU);
      if (ret < 0) {
        // fprintf(stderr, "Failed to read packet!\n");
      } else {
        // packet_dump(buf, ret);

        iphdr = (struct iphdr*)(buf + ETH_HLEN);
        dest  = iphdr->daddr;

        vsg_send(context, dest, ret, buf);
      }
    }
  }
  return NULL;
}

int main(int argc, char** argv)
{
  vmi_instance_t vmi         = {0};
  status_t status            = VMI_FAILURE;
  vmi_mode_t mode            = {0};
  vmi_init_data_t* init_data = NULL;
  int retcode                = 1;
  pthread_t packets_thread;
  unsigned long mfn;
  xc_interface* xch;
  void* memory;
  char debug_string[16];

  /* this is the VM or file that we are looking at */
  if (argc != 7) {
    fprintf(stderr, "Usage: %s <vmname> <socket> <src> <num_buffers> <domid> <net_device_name> \n", argv[0]);
    return retcode;
  }

  char* name            = argv[1];
  char* socket          = argv[2];
  char* src             = argv[3];
  int num_buffers       = atoi(argv[4]);
  uint16_t domid        = atoi(argv[5]);
  char* net_device_name = argv[6];

  vsg_setup(socket, src, num_buffers);
  printf("vsg setup done\n");

  // Context is now initialized

  fd = get_tansiv_timer_fd(context);
  printf("Got kernel module fd\n");

  if (ioctl_register_vm(fd, domid, net_device_name)) {
    fprintf(stderr, "Failed to register VM in kernel module\n");
    goto error_exit;
  };
  printf("Registered VM in kernel module\n");

  if (VMI_FAILURE == vmi_get_access_mode(NULL, (void*)name, VMI_INIT_DOMAINNAME | VMI_INIT_EVENTS, init_data, &mode)) {
    fprintf(stderr, "Failed to get access mode\n");
    goto error_exit;
  }
  printf("Accessed node in libVMI.\n");

  if (VMI_FAILURE == vmi_init(&vmi, mode, name, VMI_INIT_DOMAINNAME | VMI_INIT_EVENTS, init_data, NULL)) {
    fprintf(stderr, "Failed to init LibVMI library.\n");
    goto error_exit;
  }
  printf("LibVMI initialized.\n");

  pfd.fd     = fd;
  pfd.events = POLLIN;

  pthread_create(&packets_thread, NULL, read_packets, NULL);

  struct sigaction act;
  /* for a clean exit */
  act.sa_handler = close_handler;
  act.sa_flags   = 0;
  sigemptyset(&act.sa_mask);
  sigaction(SIGHUP, &act, NULL);
  sigaction(SIGTERM, &act, NULL);
  sigaction(SIGINT, &act, NULL);
  sigaction(SIGALRM, &act, NULL);

  vmi_event_t tansiv_deadline_event = {0};
  tansiv_deadline_event.version     = VMI_EVENTS_VERSION;
  tansiv_deadline_event.type        = VMI_EVENT_TANSIV_DEADLINE;
  tansiv_deadline_event.callback    = tansiv_deadline_callback;

  // TODO: Don't use libvmi for this hypercall
  vmi_event_t tansiv_page_event                        = {0};
  tansiv_page_event.version                            = VMI_EVENTS_VERSION;
  tansiv_page_event.type                               = VMI_EVENT_TANSIV_REGISTER_TSC_PAGE;
  tansiv_page_event.callback                           = dummy_cb;
  tansiv_page_event.tansiv_register_tsc_page_event.mfn = &mfn;

  // register events
  if (vmi_register_event(vmi, &tansiv_deadline_event) == VMI_FAILURE)
    goto error_exit;
  printf("TANSIV_DEADLINE event registered.\n");

  if (vmi_register_event(vmi, &tansiv_page_event) == VMI_FAILURE)
    goto error_exit;
  printf("TANSIV_REGISTER_TSC_PAGE event registered.\n");

  printf("mfn is %lu\n", mfn);

  xch = xc_interface_open(NULL, NULL, 0);
  if (xch == NULL)
    goto error_exit;

  printf("xen channel opened!\n");

  memory = xc_map_foreign_range(xch, DOMID_XEN, 4096, PROT_READ, mfn);
  if (memory == NULL) {
    fprintf(stderr, "Failed to map xen memory.\n");
    goto error_exit;
  }

  strcpy(debug_string, (char*)memory);
  printf("debug_string is %s\n", debug_string);

  set_tansiv_tsc_page(context, memory);

  printf("Waiting for events...\n");
  while (!interrupted) {
    status = vmi_events_listen(vmi, 0);
    if (status == VMI_FAILURE)
      printf("Failed to listen on events\n");
  }

  retcode = 0;
error_exit:
  pthread_cancel(packets_thread);
  /* TODO : vmi_clear_event must be updated to support tansiv events */
  vmi_clear_event(vmi, &tansiv_deadline_event, NULL);
  vmi_clear_event(vmi, &tansiv_page_event, NULL);

  /* cleanup any memory associated with the LibVMI instance */
  vmi_destroy(vmi);

  if (init_data) {
    free(init_data->entry[0].data);
    free(init_data);
  }

  if (xch)
    xc_interface_close(xch);

  return retcode;
}
