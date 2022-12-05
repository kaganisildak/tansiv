#include <fcntl.h>
#include <stdbool.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/file.h>
#include <sys/ioctl.h>
#include <unistd.h>

#include "tansiv-timer.h"

#define LOCK_PATH "/tmp/tansiv_timer_lock"

/* Register a VM */
int ioctl_register_vm(pid_t pid) {
    int error;
    int fd;
    struct tansiv_vm_ioctl info;
    int fd_lock;
    info.pid = pid;

    fd_lock = open(LOCK_PATH, O_CREAT, 0600);
    if (fd_lock < 0) {
        printf("Failed to open lock file\n");
    }
    flock(fd_lock, LOCK_EX);
    
    fd = open(DEVICE_PATH, O_RDWR);
    if (fd < 0) {
        printf("Failed to open device file: %s, error:%d\n", DEVICE_PATH, fd);
        exit(EXIT_FAILURE);
    }

    error = ioctl(fd, TANSIV_REGISTER_VM, &info);
    if (error < 0) {
        perror("ioctl_register_vm");
        return -1;
    }
    close(fd);
    flock(fd_lock, LOCK_UN);
    close(fd_lock);
    return error;
}

/* Set deadline for a VM */
unsigned long long int ioctl_register_deadline(pid_t pid, unsigned long long int deadline, unsigned long long int deadline_tsc) {
    int error;
    int fd;
    struct tansiv_deadline_ioctl info;
    int fd_lock;
    info.pid = pid;
    info.deadline = deadline;
    info.deadline_tsc = deadline_tsc;
    fd_lock = open(LOCK_PATH, O_CREAT, 0600);
    if (fd_lock < 0) {
        printf("Failed to open lock file\n");
    }
    flock(fd_lock, LOCK_EX);

    fd = open(DEVICE_PATH, O_RDWR);
    if (fd < 0) {
        printf("Failed to open device file: %s, error:%d\n", DEVICE_PATH, fd);
        exit(EXIT_FAILURE);
    }

    error = ioctl(fd, TANSIV_REGISTER_DEADLINE, &info);
    if (error < 0) {
        perror("ioctl_set_deadline");
        return -1;
    }
    close(fd);
    flock(fd_lock, LOCK_UN);
    close(fd_lock);
    return info.vmx_timer_value;
}

/* Register a vcpu thread */
int ioctl_register_vcpu(pid_t pid, pid_t vcpu_pid)
{
    int error;
    int fd;
    int fd_lock;
    struct tansiv_vcpu_ioctl info;
    info.pid = pid;
    info.vcpu_pid = vcpu_pid;

    fd_lock = open(LOCK_PATH, O_CREAT, 0600);
    if (fd_lock < 0) {
        printf("Failed to open lock file\n");
    }
    flock(fd_lock, LOCK_EX);

    fd = open(DEVICE_PATH, O_RDWR);
    if (fd < 0) {
        printf("Failed to open device file: %s, error:%d\n", DEVICE_PATH, fd);
        exit(EXIT_FAILURE);
    }

    error = ioctl(fd, TANSIV_REGISTER_VCPU, &info);
    if (error < 0) {
        perror("ioctl_register_vcpu");
        return -1;
    }
    close(fd);
    flock(fd_lock, LOCK_UN);
    close(fd_lock);
    return error;
}

int ioctl_init_end(pid_t pid)
{
    int error;
    int fd;
    int fd_lock;
    struct tansiv_init_end_ioctl info;
    info.pid = pid;

    fd_lock = open(LOCK_PATH, O_CREAT, 0600);
    if (fd_lock < 0) {
        printf("Failed to open lock file\n");
    }
    flock(fd_lock, LOCK_EX);

    fd = open(DEVICE_PATH, O_RDWR);
    if (fd < 0) {
        printf("Failed to open device file: %s, error:%d\n", DEVICE_PATH, fd);
        exit(EXIT_FAILURE);
    }

    error = ioctl(fd, TANSIV_INIT_END, &info);
    if (error < 0) {
        perror("ioctl_init_end");
        return -1;
    }
    close(fd);
    flock(fd_lock, LOCK_UN);
    close(fd_lock);
    return error;
}

bool ioctl_init_check(pid_t pid)
{
    int error;
    int fd;
    int fd_lock;
    struct tansiv_init_check_ioctl info;
    info.pid = pid;
    info.status = false;

    fd_lock = open(LOCK_PATH, O_CREAT, 0600);
    if (fd_lock < 0) {
        printf("Failed to open lock file\n");
    }
    flock(fd_lock, LOCK_EX);

    fd = open(DEVICE_PATH, O_RDWR);
    if (fd < 0) {
        printf("Failed to open device file: %s, error:%d\n", DEVICE_PATH, fd);
        exit(EXIT_FAILURE);
    }

    error = ioctl(fd, TANSIV_INIT_CHECK, &info);
    if (error < 0) {
        perror("ioctl_init_check");
        return -1;
    }
    close(fd);
    flock(fd_lock, LOCK_UN);
    close(fd_lock);
    return info.status;
}

unsigned long long int ioctl_scale_tsc(pid_t pid, unsigned long long int tsc)
{
    int error;
    int fd;
    int fd_lock;
    struct tansiv_scale_tsc_ioctl info;
    info.pid = pid;
    info.tsc = tsc;

    fd_lock = open(LOCK_PATH, O_CREAT, 0600);
    if (fd_lock < 0) {
        printf("Failed to open lock file\n");
    }
    flock(fd_lock, LOCK_EX);

    fd = open(DEVICE_PATH, O_RDWR);
    if (fd < 0) {
        printf("Failed to open device file: %s, error:%d\n", DEVICE_PATH, fd);
        exit(EXIT_FAILURE);
    }

    error = ioctl(fd, TANSIV_SCALE_TSC, &info);
    if (error < 0) {
        perror("ioctl_init_check");
        return -1;
    }
    close(fd);
    flock(fd_lock, LOCK_UN);
    close(fd_lock);
    return info.scaled_tsc;
}