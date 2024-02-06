#include <fcntl.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/file.h>
#include <sys/ioctl.h>
#include <unistd.h>

#include "tansiv-timer-xen.h"

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
int ioctl_register_vm(int fd, uint16_t domid, char net_device_name[IFNAMSIZ]) {
    int error;
    struct tansiv_vm_ioctl info;
    info.domid = domid;

    strncpy(info.net_device_name, net_device_name, IFNAMSIZ);

    error = ioctl(fd, TANSIV_REGISTER_VM, &info);
    if (error < 0) {
        perror("ioctl_register_vm");
        return -1;
    }
    return error;
}