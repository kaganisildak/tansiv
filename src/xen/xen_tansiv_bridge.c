/*
 * Interface between TANSIV-client and Xen by using LibVMI
 */

#include <assert.h>
#include <errno.h>
#include <inttypes.h>
#include <limits.h>
#include <math.h>
#include <poll.h>
#include <pthread.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/un.h>
#include <unistd.h>

#include <libvmi/events.h>
#include <libvmi/libvmi.h>

#include <arpa/inet.h>
#include <netinet/if_ether.h>
#include <netinet/in.h>  // enums IPPROTO ...
#include <netinet/ip.h>  // struct ip, iphdr ...
#include <netinet/udp.h> // struct udphdr...

#include <tansiv-client.h>
#include <tansiv-timer-xen.h>

#define XC_WANT_COMPAT_MAP_FOREIGN_API // For xc_map_foreign_range
#include <xenctrl.h>

#define PACKETS_MAX_SIZE 1600
#define MAX_VCPUS 64

struct vsg_context *context;
int fd;
struct pollfd pfd;

// mutex which plays the same role as the iothread mutex in QEMU versions of TANSIV
pthread_mutex_t deadline_lock;

uint64_t tsc_deadline = 0;

void tantap_vsg_receive_cb(uintptr_t arg __attribute__((unused)))
{
    uint8_t buf[PACKETS_MAX_SIZE];
    uint32_t src, dst;
    uint32_t msg_len = PACKETS_MAX_SIZE;
    while (vsg_poll(context) == 0) {
        vsg_recv(context, &src, &dst, &msg_len, buf);
        // Send the packet to the kernel module
        write(fd, buf, msg_len);
    }
}

void dummy_vsg_deadline_cb(uintptr_t arg __attribute__((unused)),
                           struct timespec deadline __attribute__((unused)))
{
}

event_response_t dummy_cb(vmi_instance_t vmi __attribute__((unused)),
                          vmi_event_t *event __attribute__((unused)))
{
    return VMI_EVENT_RESPONSE_NONE;
}

void vsg_setup(const char *socket, const char *src, uint64_t num_buffers)
{
    int vsg_argc = 8;
    char num_buffers_c[20];
    sprintf(num_buffers_c, "%ld", num_buffers);
    const char *const vsg_argv[] = {"-a", socket,        "-n", src,
                                    "-b", num_buffers_c, "-t", "1970-01-01T00:00:00"};

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
    context = vsg_init(vsg_argc, vsg_argv, NULL, tantap_vsg_receive_cb, (uintptr_t)0,
                       dummy_vsg_deadline_cb, 0);
    assert(context != NULL);
}

void start_simulation(int socket_fd)
{
    int ret;
    struct pollfd socket_pfd;
    char buf[512];
    struct sockaddr_un client_addr;
    socklen_t client_len = sizeof(client_addr);
    int client_fd;
    socket_pfd.fd = socket_fd;
    socket_pfd.events = POLLIN;

    while (true) {
        poll(&socket_pfd, 1, -1);
        if (socket_pfd.revents & POLLIN) {

            client_fd = accept(socket_fd, (struct sockaddr *)&client_addr, &client_len);
            if (client_fd == -1) {
                fprintf(stderr, "accept() failed!\n");
            }

            ret = recv(client_fd, buf, 512, 0);
            close(client_fd);
            if (ret < 0) {
                fprintf(stderr, "recv() failed!\n");
            } else {
                buf[ret] = '\0';
                printf("Received: %s\n", buf);
                if (strlen(buf) == 9) {
                    if (strncmp(buf, "vsg_start", 9) == 0) {
                        ret = vsg_start(context, NULL);
                        assert(ret == 0);
                        printf("vsg_start() successful\n");
                        break;
                    }
                }
            }
        }
    }
}

int init_socket(char *socket_name, int name_size)
{
    int socket_fd;
    struct sockaddr_un addr;
    int res;

    unlink(socket_name);

    socket_fd = socket(PF_LOCAL, SOCK_STREAM, 0);
    if (socket_fd < 0) {
        fprintf(stderr, "socket() failed!\n");
    }

    memset(&addr, 0, sizeof(addr));
    addr.sun_family = AF_LOCAL;
    strncpy(addr.sun_path, socket_name, name_size);

    res = bind(socket_fd, (struct sockaddr *)&addr, sizeof(addr));
    if (res < 0) {
        fprintf(stderr, "bind() failed!\n");
    }

    res = listen(socket_fd, 1);
    if (res < 0) {
        fprintf(stderr, "listen() failed!\n");
    }

    return socket_fd;
}

static int interrupted = 0;
void close_handler(int sig) { interrupted = sig; }

extern uint64_t deadline_handler(const struct vsg_context *context, uint64_t guest_tsc)
    __attribute__((weak));
extern int get_tansiv_timer_fd(const struct vsg_context *context) __attribute__((weak));
extern int set_tansiv_tsc_page(const struct vsg_context *context, void *memory)
    __attribute__((weak));

event_response_t tansiv_deadline_callback(vmi_instance_t vmi, vmi_event_t *event)
{
    pthread_mutex_lock(&deadline_lock);
    tsc_deadline = deadline_handler(context, 0);
    pthread_mutex_unlock(&deadline_lock);

    event->tsc_deadline = tsc_deadline;

    return VMI_EVENT_RESPONSE_NONE;
}

void packet_dump(uint8_t *buf, int size)
{
    printf("****Packet of size %d****\n", size);
    for (int i = 0; i < size; i++) {
        printf("%02x ", buf[i]);
        if ((i + 1) % 16 == 0)
            printf("\n");
    }
    printf("\n*******************\n\n");
}

void *read_packets(void *unused __attribute__((unused)))
{
    int ret;
    uint8_t buf[PACKETS_MAX_SIZE];
    struct iphdr *iphdr;
    in_addr_t dest;

    while (true) {
        poll(&pfd, 1, -1);
        if (pfd.revents & POLLIN) {
            ret = read(fd, buf, PACKETS_MAX_SIZE);
            if (ret < 0) {
                // fprintf(stderr, "Failed to read packet!\n");
            } else {
                // packet_dump(buf, ret);

                iphdr = (struct iphdr *)(buf + ETH_HLEN);
                dest = iphdr->daddr;

                // Dont send if we are handling a deadline!
                pthread_mutex_lock(&deadline_lock);
                vsg_send(context, dest, ret, buf);
                pthread_mutex_unlock(&deadline_lock);
            }
        }
    }
    return NULL;
}

int num_digits(int n)
{
    if (n < 0)
        return num_digits((n == INT_MIN) ? INT_MAX : -n);
    if (n < 10)
        return 1;
    return 1 + num_digits(n / 10);
}

int main(int argc, char **argv)
{
    vmi_instance_t vmi = {0};
    status_t status = VMI_FAILURE;
    vmi_mode_t mode = {0};
    vmi_init_data_t *init_data = NULL;
    int retcode = 1;
    pthread_t packets_thread;
    unsigned long mfn;
    xc_interface *xch;
    void *memory;
    char debug_string[16];
    char socket_name[40] = "/tmp/xen_tansiv_bridge_socket_"; // extra room for suffix
    int socket_fd;

    pthread_mutex_init(&deadline_lock, NULL);

    /* this is the VM or file that we are looking at */
    if (argc != 7) {
        fprintf(stderr,
                "Usage: %s <vmname> <socket> <src> <num_buffers> <domid> "
                "<net_device_name> \n",
                argv[0]);
        return retcode;
    }

    char *name = argv[1];
    char *socket = argv[2];
    char *src = argv[3];
    int num_buffers = atoi(argv[4]);
    uint16_t domid = atoi(argv[5]);
    char *net_device_name = argv[6];

    vsg_setup(socket, src, num_buffers);
    printf("vsg setup done\n");

    int suffix_len = num_digits((int)domid);
    printf("suffix_len is %d\n", suffix_len);

    char suffix[5] = {0}; // Note: The domid must be < 65535
    if (snprintf(suffix, 5, "%hu", domid)) {
        printf("snprintf success\n");
    }

    strncat(socket_name, suffix, suffix_len);
    printf("socket_name is %s\n", socket_name);

    socket_fd = init_socket(socket_name, 40);
    printf("socket created\n");

    start_simulation(socket_fd);

    // Context is now initialized

    fd = get_tansiv_timer_fd(context);
    printf("Got kernel module fd\n");

    if (ioctl_register_vm(fd, domid, net_device_name)) {
        fprintf(stderr, "Failed to register VM in kernel module\n");
        goto error_exit;
    };
    printf("Registered VM in kernel module\n");

    if (VMI_FAILURE == vmi_get_access_mode(NULL, (void *)name,
                                           VMI_INIT_DOMAINNAME | VMI_INIT_EVENTS,
                                           init_data, &mode)) {
        fprintf(stderr, "Failed to get access mode\n");
        goto error_exit;
    }
    printf("Accessed node in libVMI.\n");

    if (VMI_FAILURE == vmi_init(&vmi, mode, name, VMI_INIT_DOMAINNAME | VMI_INIT_EVENTS,
                                init_data, NULL)) {
        fprintf(stderr, "Failed to init LibVMI library.\n");
        goto error_exit;
    }
    printf("LibVMI initialized.\n");

    pfd.fd = fd;
    pfd.events = POLLIN;

    pthread_create(&packets_thread, NULL, read_packets, NULL);

    struct sigaction act;
    /* for a clean exit */
    act.sa_handler = close_handler;
    act.sa_flags = 0;
    sigemptyset(&act.sa_mask);
    sigaction(SIGHUP, &act, NULL);
    sigaction(SIGTERM, &act, NULL);
    sigaction(SIGINT, &act, NULL);
    sigaction(SIGALRM, &act, NULL);

    vmi_event_t tansiv_deadline_event = {0};
    tansiv_deadline_event.version = VMI_EVENTS_VERSION;
    tansiv_deadline_event.type = VMI_EVENT_TANSIV_DEADLINE;
    tansiv_deadline_event.callback = tansiv_deadline_callback;

    // TODO: Don't use libvmi for this hypercall, because it is not an event
    vmi_event_t tansiv_page_event = {0};
    tansiv_page_event.version = VMI_EVENTS_VERSION;
    tansiv_page_event.type = VMI_EVENT_TANSIV_REGISTER_TSC_PAGE;
    tansiv_page_event.callback = dummy_cb;
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

    strcpy(debug_string, (char *)memory);
    printf("debug_string is %s\n", debug_string);

    set_tansiv_tsc_page(context, memory);

    printf("Waiting for events...\n");
    while (!interrupted) {
        status = vmi_events_listen(vmi, 100);
        if (status == VMI_FAILURE)
            printf("Failed to listen on events\n");
    }

    retcode = 0;
error_exit:
    close(socket_fd);
    unlink(socket_name);
    pthread_mutex_destroy(&deadline_lock);
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
