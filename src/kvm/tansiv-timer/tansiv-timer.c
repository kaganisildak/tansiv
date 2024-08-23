#include <fcntl.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/file.h>
#include <sys/ioctl.h>
#include <unistd.h>

#include "tansiv-timer.h"

int open_device() {
    int fd;
    fd = open(DEVICE_PATH, O_RDWR);
    if (fd < 0) {
        printf("Failed to open device file: %s, error:%d\n", DEVICE_PATH, fd);
        exit(EXIT_FAILURE);
    }
    return fd;
}

void close_device(int fd) {
    close(fd);
}

/* Register a VM */
int ioctl_register_vm(int fd, pid_t pid) {
    int error;
    struct tansiv_vm_ioctl info;
    info.pid = pid;

    error = ioctl(fd, TANSIV_REGISTER_VM, &info);
    if (error < 0) {
        perror("ioctl_register_vm");
        return -1;
    }
    return error;
}

/* Set deadline for a VM */
unsigned long long int ioctl_register_deadline(int fd, unsigned long long int deadline, unsigned long long int deadline_tsc) {
    int error;
    struct tansiv_deadline_ioctl info;
    info.deadline = deadline;
    info.deadline_tsc = deadline_tsc;

    error = ioctl(fd, TANSIV_REGISTER_DEADLINE, &info);
    if (error < 0) {
        perror("ioctl_set_deadline");
        return -1;
    }
    return info.vmx_timer_value;
}

/* Register a vcpu thread */
int ioctl_register_vcpu(int fd, pid_t vcpu_pid)
{
    int error;
    struct tansiv_vcpu_ioctl info;
    info.vcpu_pid = vcpu_pid;

    error = ioctl(fd, TANSIV_REGISTER_VCPU, &info);
    if (error < 0) {
        perror("ioctl_register_vcpu");
        return -1;
    }
    return error;
}

int ioctl_init_end(int fd)
{
    int error;
    struct tansiv_init_end_ioctl info;

    error = ioctl(fd, TANSIV_INIT_END, &info);
    if (error < 0) {
        perror("ioctl_init_end");
        return -1;
    }
    return error;
}

bool ioctl_init_check(int fd)
{
    int error;
    struct tansiv_init_check_ioctl info;
    info.status = false;

    error = ioctl(fd, TANSIV_INIT_CHECK, &info);
    if (error < 0) {
        perror("ioctl_init_check");
        return -1;
    }
    return info.status;
}

int ioctl_register_tap(int fd, const char net_device_name[IFNAMSIZ])
{
    int error;
    struct tansiv_register_tap_ioctl info;
    strncpy(info.net_device_name, net_device_name, IFNAMSIZ);

    error = ioctl(fd, TANSIV_REGISTER_TAP, &info);
    if (error < 0) {
        perror("ioctl_register_tap");
        return -1;
    }
    return error;
}