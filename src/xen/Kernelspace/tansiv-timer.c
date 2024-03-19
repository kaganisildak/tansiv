#include <linux/cpu.h>
#include <linux/device.h>
#include <linux/fs.h>
#include <linux/highmem.h>
#include <linux/hrtimer.h>
#include <linux/if.h>
#include <linux/in.h>
#include <linux/init.h>
#include <linux/ip.h>
#include <linux/kernel.h>
#include <linux/kobject.h>
#include <linux/mm.h>
#include <linux/module.h>
#include <linux/netdevice.h>
#include <linux/pid.h>
#include <linux/poll.h>
#include <linux/sched/signal.h>
#include <linux/skbuff.h>
#include <linux/spinlock.h>
#include <linux/string.h>
#include <linux/sysfs.h>
#include <linux/tcp.h>
#include <linux/timekeeping.h>
#include <linux/uaccess.h>
#include <linux/udp.h>
#include <linux/wait.h>
#include <linux/workqueue.h>

#include <net/tcp.h>
#include <net/udp.h>

#include <xen/interface/xen.h>

#include "../include/tansiv-timer-xen.h"

#define DEVICE_NAME "tansiv_dev"
#define LOGS_BUFFER_SIZE 500
#define LOGS_LINE_SIZE 500
#define PACKETS_BUFFER_SIZE 1000
#define PACKETS_MAX_SIZE 1600

#define FORBIDDEN_MMAP_FLAG (VM_WRITE | VM_EXEC | VM_SHARED)

/* Global variables */

static struct class *cls;                    // Class for the device
static struct file *logs_file;               // file descriptor for the VM logs
static struct circular_buffer logs_buffer;   // circular buffer for the VM logs
static struct work_struct logs_work;         // work struct to write in the logs file
static spinlock_t logs_buffer_lock;          // spinlock for the logs buffer
static DECLARE_WAIT_QUEUE_HEAD(device_wait); // waitqueue

/* Data structures and enums */

/* VM TSC infos */
struct tansiv_vm_tsc_infos {
    u64 tsc_offset;
    u64 tsc_scaling_ratio;
};

/* Circular buffer for network packets and logs */
struct circular_buffer {
    void *buffer;
    void *buffer_end;
    void *head;
    void *tail;
    ssize_t size;
    ssize_t used;
    ssize_t item_size;
};

/* struct representing a packet */
struct tansiv_packet {
    ktime_t timestamp;
    unsigned char data[PACKETS_MAX_SIZE];
    unsigned int size;
};

/* struct representing a VM */
struct tansiv_vm {
    domid_t domid;                  // VM domain id
    char net_device_name[IFNAMSIZ]; // VM network interface name
    struct net_device *dev;         // Net device associated with this VM
    struct circular_buffer packets; // Buffer of intercepted network packets
    spinlock_t packets_lock; // spinlock for the packet buffer
};

/* Init's network namespace */
extern struct net init_net;

/* Init circular buffer */
static void cb_init(struct circular_buffer *cb, ssize_t size, ssize_t item_size)
{
    cb->buffer = kmalloc(size * item_size, GFP_KERNEL);
    if (cb->buffer == NULL) {
        pr_err("tansiv-timer: Failed to allocate memory of size %ld for the buffer\n",
               size * item_size);
        return;
    }
    cb->buffer_end = cb->buffer + size * item_size;
    cb->head = cb->buffer;
    cb->tail = cb->buffer;
    cb->size = size;
    cb->used = 0;
    cb->item_size = item_size;
}

/* Free circular buffer */
static void cb_free(struct circular_buffer *cb) { kfree(cb->buffer); }

/* Push an item in a circular buffer */
static void cb_push(struct circular_buffer *cb, void *item)
{
    if (cb->used == cb->size) {
        pr_err("tansiv-timer: Buffer is full\n");
    }
    memcpy(cb->head, item, cb->item_size);
    cb->head = cb->head + cb->item_size;
    if (cb->head == cb->buffer_end) {
        cb->head = cb->buffer;
    }
    cb->used++;
}

/* Pop an item from the circular buffer */
static void cb_pop(struct circular_buffer *cb, void *item)
{
    if (cb->used == 0) {
        pr_err("tansiv-timer: Buffer is empty\n");
        return;
    }
    memcpy(item, cb->tail, cb->item_size);
    cb->tail = cb->tail + cb->item_size;
    if (cb->tail == cb->buffer_end) {
        cb->tail = cb->buffer;
    }
    cb->used--;
}

/* Worker to write in the logs file */
void write_logs(struct work_struct *unused)
{
    char buffer[LOGS_LINE_SIZE];
    while (logs_buffer.used > 0) {
        spin_lock_irq(&logs_buffer_lock);
        cb_pop(&logs_buffer, buffer);
        spin_unlock_irq(&logs_buffer_lock);
        if (kernel_write(logs_file, buffer, strlen(buffer), 0) < 0) {
            pr_err("tansiv-timer: Error while writing logs\n");
        }
    }
}

/* Initialize a new VM */
struct tansiv_vm *init_vm(void)
{
    struct tansiv_vm *vm = kmalloc(sizeof(struct tansiv_vm), GFP_KERNEL);
    if (vm == NULL) {
        pr_err("tansiv-timer: Failed to allocate memory for the vm\n");
        return NULL;
    }
    vm->domid = 0;
    cb_init(&vm->packets, PACKETS_BUFFER_SIZE, sizeof(struct sk_buff *));
    spin_lock_init(&vm->packets_lock);

    return vm;
}

/* Free a VM */
void free_vm(struct tansiv_vm *vm)
{
    struct sk_buff *skb;
    while (vm->packets.used > 0) {
        cb_pop(&vm->packets, &skb);
        if (skb != NULL)
            kfree_skb(skb);
        else
            pr_info("tansiv-timer: free_vm: skb is NULL!");
    }
    cb_free(&vm->packets);
}

/* Open the device */
static int device_open(struct inode *inode, struct file *file)
{
    struct tansiv_vm *vm;
    pr_info("tansiv-timer: device_open(%p)\n", file);

    try_module_get(THIS_MODULE);
    vm = init_vm();
    if (vm == NULL) {
        pr_err("tansiv-timer: failed to intialize VM\n");
        return -EFAULT;
    }
    file->private_data = vm;
    pr_info("tansiv-timer: device opened\n");
    return 0;
}

/* Close the device */
static int device_release(struct inode *inode, struct file *file)
{
    struct tansiv_vm *vm = file->private_data;
    pr_info("tansiv-timer: device_release(%p, %p)\n", inode, file);
    free_vm(vm);
    kfree(vm);

    module_put(THIS_MODULE);
    pr_info("tansiv-timer: device closed\n");
    return 0;
}

/* Callback to intercept xen packets */
void xen_cb(void *buff)
{
    struct sk_buff *skb;
    struct net_device *dev;
    struct tansiv_vm *vm;

    if (buff == NULL) {
        pr_warn("tansiv-timer: intercepted NULL skb!");
        return;
    }

    skb = (struct sk_buff *)buff;

    if (skb == NULL) {
        pr_warn("tansiv-timer: skb is NULL!");
        return;
    }

    if (skb->data - ETH_HLEN < skb->head) {
        pr_warn("tansiv-timer: skb ethernet header is not in headroom!");
        return;
    }

    // Put back the ethernet header in the linear part of the skb
    skb_push(skb, ETH_HLEN);

    /* Find the corresponding net_device */
    dev = skb->dev;
    if (dev == NULL) {
        pr_warn("tansiv-timer: skb->dev is NULL!");
        return;
    }

    /* Find the vm struct with this net_device */
    vm = dev->tansiv_vm;
    if (vm == NULL) {
        pr_warn("tansiv-timer: skb->dev->vm is NULL!");
        return;
    }

    /* Forward the packet to userspace */
    spin_lock(&vm->packets_lock);
    cb_push(&vm->packets, &skb);
    wake_up_interruptible(&device_wait);
    spin_unlock(&vm->packets_lock);
}

struct net_device *find_netdev(char *name)
{
    struct net_device *dev;
    for_each_netdev(&init_net, dev)
    {
        if (strcmp(dev->name, name) == 0) {
            return dev;
        }
    }
    return NULL;
}

/* IOCTL handler */
static long device_ioctl(struct file *file, unsigned int ioctl_num,
                         unsigned long ioctl_param)
{
    // pr_info("device_ioctl(%p, %u, %lu)\n", file, ioctl_num, ioctl_param);
    struct tansiv_vm *vm = file->private_data;
    switch (ioctl_num) {
    case TANSIV_REGISTER_VM: {
        struct tansiv_vm_ioctl __user *tmp = (struct tansiv_vm_ioctl *)ioctl_param;
        struct tansiv_vm_ioctl _vm_info;
        struct net_device *dev;

        if (copy_from_user(&_vm_info, tmp, sizeof(struct tansiv_vm_ioctl))) {
            return -EFAULT;
        }
        pr_info("tansiv-timer: TANSIV_REGISTER_VM: domid = %d\n", _vm_info.domid);

        vm->domid = _vm_info.domid;
        strncpy(vm->net_device_name, _vm_info.net_device_name, IFNAMSIZ);

        dev = find_netdev(vm->net_device_name);
        if (dev == NULL) {
            pr_warn("tansiv-timer: TANSIV_REGISTER_VM: unknown netdev\n");
            return -ENODEV;
        }
        vm->dev = dev;

        dev->tansiv_cb = xen_cb;
        dev->tansiv_vm = vm;

        pr_info("tansiv-timer: TANSIV_REGISTER_VM: successful\n");
        break;
    }
    default: {
        pr_err("tansiv-timer: Unknown ioctl command\n");
        return -EINVAL;
    }
    }
    return 0;
}

static ssize_t device_do_read(struct tansiv_vm *vm, struct iov_iter *to)
{
    struct sk_buff *skb;
    ssize_t ret = 0;
    
    if (vm->packets.used > 0) {
        cb_pop(&vm->packets, &skb);
        if (skb != NULL) {
            ret = skb_copy_datagram_iter(skb, 0, to, skb->len);
            ret = ret ? ret : skb->len;
            kfree_skb(skb);
        } else
            pr_warn("tansiv-timer: device_do_read: skb is NULL!");
    }

    return ret;
}

/* read (kernel -> userspace) */
static ssize_t device_read_iter(struct kiocb *iocb, struct iov_iter *to)
{
    ssize_t ret;
    struct file *file = iocb->ki_filp;
    struct tansiv_vm *vm = file->private_data;
    spin_lock_irq(&vm->packets_lock);
    if (vm->packets.used == 0) {
        ret = -EAGAIN;
    }
    else {
        ret = device_do_read(vm, to);
    }
    spin_unlock_irq(&vm->packets_lock);

    return ret;
}

/* Inspired from tap_get_user of tap.c */
static ssize_t device_do_write(struct tansiv_vm *vm, struct iov_iter *from)
{
    struct sk_buff *skb;
    enum skb_drop_reason drop_reason;
    struct iphdr *ip_h;
    struct udphdr *udp_h;
    struct tcphdr *tcp_h;
    unsigned int tcp_len;
    unsigned int udp_len;
    int err = 0;
    unsigned long total_len = iov_iter_count(from);
    unsigned long len = total_len;
    if (len < ETH_HLEN) {
        pr_warn("tansiv-timer: Packet smaller than the eth header size!\n");
        return -EINVAL;
    }

    skb = alloc_skb_with_frags(NET_SKB_PAD + NET_IP_ALIGN + len, 0,
                               PAGE_ALLOC_COSTLY_ORDER, &err, GFP_ATOMIC | __GFP_NOWARN);
    if (skb == NULL) {
        pr_warn("tansiv-timer: Error when allocating skb\n");
        return -1;
    }

    skb_reserve(skb, NET_SKB_PAD + NET_IP_ALIGN);
    skb_put(skb, len);
    skb->data_len = 0;

    err = skb_copy_datagram_from_iter(skb, 0, from, len);
    if (err) {
        pr_warn("tansiv-timer: Error when copying packet from userspace!\n");
        drop_reason = SKB_DROP_REASON_SKB_UCOPY_FAULT;
        kfree_skb_reason(skb, drop_reason);
        return err;
    }

    skb_set_network_header(skb, ETH_HLEN);
    skb_reset_mac_header(skb);
    skb->protocol = eth_hdr(skb)->h_proto;

    rcu_read_lock();
    skb->dev = vm->dev;
    skb->pkt_type = PACKET_OTHERHOST;

    skb_probe_transport_header(skb);

    if (htons(skb->protocol) == ETH_P_IP) {
        ip_h = ip_hdr(skb);
        switch (ip_h->protocol) {
        case IPPROTO_TCP:
            tcp_h = tcp_hdr(skb);
            tcp_len = skb->len - ip_hdrlen(skb) - ETH_HLEN;
            tcp_h->check = 0;
            tcp_h->check = tcp_v4_check(tcp_len, ip_h->saddr, ip_h->daddr,
                                        csum_partial((char *)tcp_h, tcp_len, 0));
            break;
        case IPPROTO_UDP:
            udp_h = udp_hdr(skb);
            udp_len = skb->len - ip_hdrlen(skb) - ETH_HLEN;
            udp_h->check = 0;
            udp_h->check = udp_v4_check(udp_len, ip_h->saddr, ip_h->daddr,
                                        csum_partial((char *)udp_h, udp_len, 0));
            break;
        default:
            break;
        }
    }

    dev_queue_xmit(skb);
    rcu_read_unlock();

    return len;
}

/* write (userspace -> kernel) */
static ssize_t device_write_iter(struct kiocb *iocb, struct iov_iter *from)
{
    struct file *file = iocb->ki_filp;
    struct tansiv_vm *vm = file->private_data;
    return device_do_write(vm, from);
}

/* poll */
static __poll_t device_poll(struct file *file, poll_table *wait)
{
    struct tansiv_vm *vm = file->private_data;
    __poll_t mask = EPOLLERR;

    if (vm == NULL)
        goto out;

    mask = 0;
    poll_wait(file, &device_wait, wait);
    if (vm->packets.used > 0)
        mask |= POLLIN | EPOLLRDNORM;

out:
    return mask;
}

/* File operations */
static struct file_operations fops = {
    .read_iter = device_read_iter,
    .write_iter = device_write_iter,
    .unlocked_ioctl = device_ioctl,
    .open = device_open,
    .release = device_release,
    .poll = device_poll,
};

/* sysfs file to export the tsc frequency
 * Exported in /sys/devices/system/cpu/tsc_khz , in kHz
 */
static ssize_t tsc_khz_show(struct kobject *kobj, struct kobj_attribute *attr, char *buf)
{
    return sprintf(buf, "%u\n", tsc_khz);
}

struct kobj_attribute tsc_khz_attr = __ATTR_RO(tsc_khz);

/* Initialize the module */
static int __init tansiv_timer_init(void)
{
    char *s;
    int error = 0;
    pr_info("tansiv-timer: starting initialization\n");

    error = register_chrdev(MAJOR_NUM, DEVICE_NAME, &fops);

    if (error < 0) {
        pr_err("tansiv-timer: failed to register device\n");
        return error;
    }

    cls = class_create(THIS_MODULE, DEVICE_FILE_NAME);
    device_create(cls, NULL, MKDEV(MAJOR_NUM, 0), NULL, DEVICE_FILE_NAME);

    pr_info("tansiv-timer: Device created on /dev/%s\n", DEVICE_FILE_NAME);

    /* Logs */
    s = kasprintf(GFP_KERNEL, "/tmp/tansiv_kernel.csv");
    logs_file = filp_open(s, O_CREAT | O_WRONLY | O_APPEND, 0644);

    pr_info("tansiv-timer: starting circular buffer initialization\n");
    cb_init(&logs_buffer, LOGS_BUFFER_SIZE, sizeof(char[LOGS_LINE_SIZE]));

    spin_lock_init(&logs_buffer_lock);
    INIT_WORK(&logs_work, write_logs);

    if (!sysfs_create_file(&cpu_subsys.dev_root->kobj, &tsc_khz_attr.attr))
        pr_info("tansiv-timer: tsc frequency exported in sysfs");
    else
        pr_warn("tansiv-timer: tsc frequency failed to be exported in sysfs");

    pr_info("tansiv-timer: successfully initialized\n");
    return error;
}

/* Cleanup the module */
static void __exit cancel_tansiv_timer(void)
{
    /* Device cleaning */
    device_destroy(cls, MKDEV(MAJOR_NUM, 0));
    class_destroy(cls);
    unregister_chrdev(MAJOR_NUM, DEVICE_FILE_NAME);
    /* Circular buffer */
    cb_free(&logs_buffer);
    /* Sysfs */
    sysfs_remove_file(&cpu_subsys.dev_root->kobj, &tsc_khz_attr.attr);
    pr_info("tansiv-timer: Exit success\n");
}

module_init(tansiv_timer_init);
module_exit(cancel_tansiv_timer);

MODULE_LICENSE("Dual BSD/GPL");
MODULE_AUTHOR("Léo Cosseron");
MODULE_DESCRIPTION("Timers for TANSIV (Xen version)");
MODULE_VERSION("0.1");