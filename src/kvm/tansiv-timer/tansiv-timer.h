#ifndef TANSIV_TIMER_H
#define TANSIV_TIMER_H

#include <linux/ioctl.h>
#include <sys/types.h>

/* Major device number */
#define MAJOR_NUM 100

#define TANSIV_REGISTER_VM _IOW(MAJOR_NUM, 0, int)
/* _IOW : ioctl command to write information from a user program to the kernel
module
 * MAJOR_NUM : major number of the device
 * 0 is the number of the command
 * Last argument is the type to get from the process to the kernel module
 */
#define TANSIV_REGISTER_DEADLINE _IOWR(MAJOR_NUM, 1, int)

#define TANSIV_REGISTER_VCPU _IOW(MAJOR_NUM, 2, int)

#define TANSIV_INIT_END _IOW(MAJOR_NUM, 3, int)

#define TANSIV_INIT_CHECK _IOWR(MAJOR_NUM, 4, int)


#define DEVICE_FILE_NAME "tansiv_dev"
#define DEVICE_PATH "/dev/tansiv_dev"

/* IOCTL parameters */

/* TANSIV_REGISTER_VM */
struct tansiv_vm_ioctl {
    pid_t pid;
};

/* TANSIV_REGISTER_DEADLINE */
struct tansiv_deadline_ioctl {
    /* Arguments */
    unsigned long long int deadline; // Time until the next deadline (ns)
    unsigned long long int deadline_tsc; // Time until the next deadline (TSC ticks)
    /* Results */
    unsigned long long int vmx_timer_value; // Value stored in the VMX preemption timer
};

/* TANSIV_REGISTER_VCPU */
struct tansiv_vcpu_ioctl {
    pid_t vcpu_pid; // pid of the vcpu thread
};

struct tansiv_init_end_ioctl {
};

struct tansiv_init_check_ioctl {
    bool status; // true if the initialization is done
};

int ioctl_register_vm(int fd, pid_t pid);
unsigned long long int ioctl_register_deadline(int fd, unsigned long long int deadline, unsigned long long int deadline_tsc);
int ioctl_register_vcpu(int fd, pid_t vcpu_pid);
int ioctl_init_end(int fd);
bool ioctl_init_check(int fd);

#endif