#ifndef TANSIV_TIMER_H
#define TANSIV_TIMER_H

#include <linux/if.h>
#include <linux/ioctl.h>
#include <linux/types.h>

/* Major device number */
#define MAJOR_NUM 100

#define TANSIV_REGISTER_VM _IOW(MAJOR_NUM, 0, int)
/* _IOW : ioctl command to write information from a user program to the kernel
module
 * MAJOR_NUM : major number of the device
 * 0 is the number of the command
 * Last argument is the type to get from the process to the kernel module
 */

#define DEVICE_FILE_NAME "tansiv_dev"
#define DEVICE_PATH "/dev/tansiv_dev"

/* IOCTL parameters */

/* TANSIV_REGISTER_VM */
struct tansiv_vm_ioctl {
  uint16_t domid;
  char net_device_name[IFNAMSIZ];
};

int ioctl_register_vm(int fd, uint16_t domid, char net_device_name[IFNAMSIZ]);

#endif