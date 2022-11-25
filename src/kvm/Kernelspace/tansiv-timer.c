#include <linux/device.h>
#include <linux/fs.h>
#include <linux/hrtimer.h>
#include <linux/init.h>
#include <linux/kernel.h>
#include <linux/kobject.h>
#include <linux/kvm_host.h>
#include <linux/module.h>
#include <linux/spinlock.h>
#include <linux/pid.h>
#include <linux/sched/signal.h>
#include <linux/string.h>
#include <linux/sysfs.h>
#include <linux/timekeeping.h>
#include <linux/uaccess.h>
#include <linux/workqueue.h>

#include <asm/kvm_host.h>

#include "../include/tansiv-timer.h"

#define DEVICE_NAME "tansiv_dev"
#define DEFAULT_NUMBER_VCPUS 8
#define DEFAULT_NUMBER_VMS 10
#define LOGS_BUFFER_SIZE 500
#define LOGS_LINE_SIZE  500


/* Data structures and enums */

enum {
    DEV_NOT_USED = 0,
    DEV_EXCLUSIVE_OPEN = 1,
};

/* Array of pid structs */
struct struct_pid_array {
    struct pid *array;
    ssize_t size;
    ssize_t used;
};

/* Circular buffer for the logs */
struct circular_buffer {
    void *buffer;
    void *buffer_end;
    void *head;
    void *tail;
    ssize_t size;
    ssize_t used;
    ssize_t item_size;
};

/* Init circular buffer */
static void cb_init(struct circular_buffer *cb, ssize_t size, ssize_t item_size) {
    cb->buffer = kmalloc(size * item_size, GFP_KERNEL);
    if (cb->buffer == NULL) {
        pr_err("tansiv-timer: Failed to allocate memory for the logs buffer\n");
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
static void cb_free(struct circular_buffer *cb) {
    kfree(cb->buffer);
}

/* Push an item in a circular buffer */
static void cb_push(struct circular_buffer *cb, void *item) {
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
static void cb_pop(struct circular_buffer *cb, void *item) {
    if (cb->used == 0) {
        pr_err("tansiv-timer: Buffer is empty\n");
    }
    memcpy(item, cb->tail, cb->item_size);
    cb->tail = cb->tail + cb->item_size;
    if (cb->tail == cb->buffer_end) {
        cb->tail = cb->buffer;
    }
    cb->used--;
}

/* Internal struct representing a VM */
struct tansiv_vm {
    struct pid pid; // VM pid
    struct hrtimer timer; // VM timer
    struct pid handler_pid; // VM handler pid
    struct struct_pid_array vcpus_pids; // Array of vcpu pids
    bool init_status; // true if the VM is fully initialized
    unsigned long long int deadline; // deadline of the VM (Cumulated)
    u64 tsc_offset; // TSC offset of the VM
    u64 tsc_scaling_ratio; // TSC scaling ratio of the VM
    u64 deadline_tsc; // Duration of current slot, in tsc ticks
    u64 timer_start; // tsc value at which the timer was started (estimated by rdtsc around hrtimer_start)
    u64 lapic_tsc_deadline; // tsc value stored in the tsc-deadline register
};

/* tansiv_vm dynamic array */
struct tansiv_vm_array {
    struct tansiv_vm *array;
    ssize_t size;
    ssize_t used;
};

/* Global variables */

static struct class *cls; // Class for the device
static struct tansiv_vm_array tansiv_vm_array; // Global array of VMs
static struct file *logs_file; // file descriptor for the VM logs
static struct circular_buffer logs_buffer; // circular buffer for the VM logs
static struct work_struct logs_work; // work struct to write in the logs file
static spinlock_t logs_buffer_lock; // spinlock for the logs buffer

/* Initialize the tansiv_vm_array */
static int init_tansiv_vm_array(struct tansiv_vm_array *array, ssize_t size)
{
    array->array = kmalloc(sizeof(struct tansiv_vm) * size, GFP_KERNEL);
    if (array->array == NULL) {
        pr_err("tansiv-timer: failed to allocate memory for the tansiv vm array");
        return -ENOMEM;
    }
    array->size = size;
    array->used = 0;
    return 0;
}

/* Insert a new VM in the tansiv_vm_array */
static int insert_tansiv_vm_array(struct tansiv_vm_array *array, struct tansiv_vm *vm)
{
   if (array->used == array->size) {
       array->array = krealloc(array->array, sizeof(struct tansiv_vm) * (2*array->size + 1), GFP_KERNEL);
       if (array->array == NULL) {
           return -ENOMEM;
       }
       array->size = 2*array->size + 1;
   }
   
   array->array[array->used++] = *vm;
   return 0;
}

/* Find a VM in the tansiv_vm_array with a given pid */
static struct tansiv_vm *find_tansiv_vm_array(struct tansiv_vm_array *array, pid_t pid)
{
    ssize_t i;
    for (i = 0; i < array->used; i++) {
        if (pid_nr(&array->array[i].pid) == pid) {
            return &array->array[i];
        }
    }
    return NULL;
}

/* Init an array of struct pids */
static int init_struct_pid_array(struct struct_pid_array *array, ssize_t size)
{
    array->array = kmalloc(sizeof(struct pid) * size, GFP_KERNEL);
    if (array->array == NULL) {
        pr_err("tansiv-timer: failed to allocate memory for the vcpu pids array");
        return -ENOMEM;
    }
    array->size = size;
    array->used = 0;
    return 0;
}


/* Register a pid */
static int register_target_pid(pid_t pid, struct pid *target_pid)
{
    rcu_read_lock();
    *target_pid = *find_get_pid(pid);
    rcu_read_unlock();
    if (!target_pid) {
	    return -ENOENT;
    }
    return 0;
}

/* Unregister a pid */
static void unregister_target_pid(struct pid* target_pid)
{
    put_pid(target_pid);
}

/* Insert a new pid in a struct_pid array */
static int insert_struct_pid_array(struct struct_pid_array *array, pid_t pid)
{
    int error;
    if (array->used == array->size) {
        array->array = krealloc(array->array, sizeof(struct pid) * (2*array->size + 1), GFP_KERNEL);
        if (array->array == NULL) {
            return -ENOMEM;
        }
        array->size = 2*array->size + 1;
    }
    error = register_target_pid(pid, &array->array[array->used]);
    return error;
}

/* Is the device open now ? */
static atomic_t already_open = ATOMIC_INIT(DEV_NOT_USED);

/* Handler for the hrtimers */
static enum hrtimer_restart timer_handler(struct hrtimer *timer)
{
    char buffer[LOGS_LINE_SIZE];
    // unsigned long long int tsc = rdtsc();
    unsigned long long int programmed_tsc;
    struct tansiv_vm *vm = container_of(timer, struct tansiv_vm, timer);

    /* Logs */
    if (logs_buffer.size == logs_buffer.used) {
        pr_err("tansiv-timer: Buffer is full\n");
    }
    if (vm->lapic_tsc_deadline == timer->tsc_deadline)
    {
        // The tsc_deadline value was not updated for some reason, for ex
        // hrtimer already expired when it was processed by the kernel
        // In this case use the value computed by the timer start date + length
        // of the slot
        // Start date is the average of two rdtsc around hrtimer_start
        vm->lapic_tsc_deadline = vm->timer_start + vm->deadline_tsc;
        // pr_info("tansiv-timer: tsc_deadline not updated, using fallback value of %llu\n", vm->lapic_tsc_deadline);
    }
    else
    {
        vm->lapic_tsc_deadline = timer->tsc_deadline;
    }
    programmed_tsc = kvm_scale_tsc(vm->lapic_tsc_deadline, vm->tsc_scaling_ratio) + vm->tsc_offset;

    // pr_info("timer-handler;%d;%d;%lld;%lld;%llu;%llu\n",
                    // pid_nr(vm->pid),
                    // raw_smp_processor_id(),
                    // timer->_softexpires,
                    // ktime_get(),
                    // programmed_tsc,
                    // vm->deadline);
    sprintf(buffer, "timer-handler;%d;%d;%lld;%lld;%llu;%llu\n",
                    pid_nr(&vm->pid),
                    raw_smp_processor_id(),
                    timer->_softexpires,
                    ktime_get(),
                    programmed_tsc,
                    vm->deadline);
    spin_lock_irq(&logs_buffer_lock);
    cb_push(&logs_buffer, buffer);
    spin_unlock_irq(&logs_buffer_lock);
    schedule_work(&logs_work);
    // pr_info("tansiv-timer: Timer expired. CPU: %d ; VM: %d ; VM deadline: %llu ; hrtimer deadline: %lld ;  programmed tsc: %llu; diff: %llu \n",
            // raw_smp_processor_id(),
            // pid_nr(&vm->pid),
            // vm->deadline,
            // timer->_softexpires,
            // programmed_tsc,
            // tsc-vm->lapic_tsc_deadline
            // );
    kvm_request_immediate_exit(pid_nr(&vm->pid));
    return HRTIMER_NORESTART;
}

/* Worker to write in the logs file */
void write_logs(struct work_struct *unused)
{
    char buffer[LOGS_LINE_SIZE];
    //pr_info("tansiv-timer: Writing logs, used:%ld\n", logs_buffer->used);
    while (logs_buffer.used > 0)
    {
        spin_lock_irq(&logs_buffer_lock);
        cb_pop(&logs_buffer, buffer);
        spin_unlock_irq(&logs_buffer_lock);
        if (kernel_write(logs_file, buffer, strlen(buffer), 0) < 0) {
            pr_err("tansiv-timer: Error while writing logs\n");
        }
    }
}

/* Initialize a new VM */
void init_vm(int pid)
{
    int error;
    struct tansiv_vm vm;

    hrtimer_init(&vm.timer, CLOCK_REALTIME, HRTIMER_MODE_REL_PINNED_HARD);
    vm.timer.function = &timer_handler;
    vm.timer.log_deadline = 1;

    error = register_target_pid(pid, &vm.pid);
    if (error) {
        unregister_target_pid(&vm.pid);
        pr_err("Error while registering target pid\n");
        return;
    }

    vm.init_status = false;
    vm.deadline = 0;
    vm.tsc_scaling_ratio = 0;
    vm.tsc_offset = 0;
    vm.deadline_tsc = 0;
    vm.timer_start = 0;
    vm.lapic_tsc_deadline = 0;

    init_struct_pid_array(&vm.vcpus_pids, DEFAULT_NUMBER_VCPUS);
    error = insert_tansiv_vm_array(&tansiv_vm_array, &vm);
    if (error < 0) {
        pr_err("Error while inserting VM in array\n");
    }
}

/* Free a VM */
void free_vm(struct tansiv_vm *vm)
{
    ssize_t i;
    hrtimer_cancel(&vm->timer);

    unregister_target_pid(&vm->pid);
    
    unregister_target_pid(&vm->handler_pid);
    
    for (i = 0; i < vm->vcpus_pids.used; i++) {
        unregister_target_pid(&vm->vcpus_pids.array[i]);
    }
    kfree(vm->vcpus_pids.array);
}

/* Free the tansiv_vm_array */
static void free_tansiv_vm_array(struct tansiv_vm_array *array)
{
    ssize_t i;
    for (i = 0; i < array->used; i++) {
        free_vm(&array->array[i]);
    }
    kfree(array->array);
}

/* Open the device */
static int device_open(struct inode *inode, struct file *file)
{
    //pr_info("device_open(%p)\n", file);

    /* Only one process at a time */
    if (atomic_cmpxchg(&already_open, DEV_NOT_USED, DEV_EXCLUSIVE_OPEN))
        return -EBUSY;
    try_module_get(THIS_MODULE);
    //pr_info("device opened\n");
    return 0;
}

/* Close the device */
static int device_release(struct inode *inode, struct file *file)
{
    // pr_info("device_release(%p, %p)\n", inode, file);

    atomic_set(&already_open, DEV_NOT_USED);

    module_put(THIS_MODULE);
    // pr_info("device closed\n");
    return 0;
}

/* IOCTL handler */
static long device_ioctl(struct file *file, unsigned int ioctl_num, unsigned long ioctl_param)
{
    //pr_info("device_ioctl(%p, %u, %lu)\n", file, ioctl_num, ioctl_param);
    switch (ioctl_num) {
        case TANSIV_REGISTER_VM: {
            struct tansiv_vm_ioctl __user *tmp = (struct tansiv_vm_ioctl *)ioctl_param;
            struct tansiv_vm_ioctl _vm_info;

            if (copy_from_user(&_vm_info, tmp, sizeof(struct tansiv_vm_ioctl))) {
                return -EFAULT;
            }

            pr_info("TANSIV_REGISTER_VM: pid = %d\n", _vm_info.pid);
            init_vm(_vm_info.pid);
            break;
        }
        case TANSIV_REGISTER_DEADLINE: {
            struct tansiv_vm *vm;
            struct tansiv_deadline_ioctl __user *tmp = (struct tansiv_deadline_ioctl *)ioctl_param;
            struct tansiv_deadline_ioctl deadline;
            int cpu;
            char buffer[LOGS_LINE_SIZE];
            //int err;
            unsigned long long int tsc_before;
            unsigned long long int tsc_after;
            unsigned long long int tsc_before_guest;
            unsigned long long int tsc_after_guest;
            bool expired;
            unsigned long long int vmx_timer_value;
            unsigned long long int last_deadline_tsc;
            unsigned long long int vmenter_guest_tsc;

            if (copy_from_user(&deadline, tmp, sizeof(struct tansiv_deadline_ioctl))) {
                return -EFAULT;
            }

            vm = find_tansiv_vm_array(&tansiv_vm_array, deadline.pid);
            if (vm == NULL) {
                pr_err("tansiv-timer: TANSIV_REGISTER_DEADLINE: vm %d not found\n", deadline.pid);
                return -EINVAL;
            }
            last_deadline_tsc = vm->deadline_tsc;
            vmenter_guest_tsc = deadline.vmenter_guest_tsc;
            
            vm->deadline += deadline.deadline;
            vm->deadline_tsc = deadline.deadline_tsc;
            cpu = raw_smp_processor_id();
            if (hrtimer_active(&vm->timer)) {
                pr_err("tansiv-timer: error, timer of vm %d is already active", deadline.pid);
            }
            tsc_before = rdtsc();
            // hrtimer_start(&vm->timer, ns_to_ktime(deadline.deadline), HRTIMER_MODE_REL_PINNED_HARD);
            tsc_after = rdtsc();

            vm->tsc_offset = kvm_get_tsc_offset(pid_nr(&vm->pid));
            vm->tsc_scaling_ratio = kvm_get_tsc_scaling_ratio(pid_nr(&vm->pid));
            // Average of both TSC values
            vm->timer_start = (tsc_before + tsc_after) >> 1;

            vmx_timer_value = vmenter_guest_tsc + last_deadline_tsc + vm->deadline_tsc;
            expired = kvm_set_preemption_timer(pid_nr(&vm->pid), vmx_timer_value);
            deadline.vmx_timer_value = vmx_timer_value;

            // pr_info("tansiv-timer: loading value %llu to set the VMX Preemption Timer. Deadline_tsc: %llu \n", vmx_timer_value, vm->deadline_tsc);
            
            tsc_before_guest = kvm_scale_tsc(tsc_before, vm->tsc_scaling_ratio) + vm->tsc_offset;
            tsc_after_guest = kvm_scale_tsc(tsc_after, vm->tsc_scaling_ratio) + vm->tsc_offset;

            // pr_info("tansiv-timer: TANSIV_REGISTER_DEADLINE: Starting hrtimer. CPU: %d ; VM: %d ; deadline : %llu ; tsc before: %llu; tsc after: %llu; scaling_ratio: %llu; offset: %llu; deadline value: %llu \n",
                    // cpu,
                    // deadline.pid,
                    // vm->deadline,
                    // tsc_before_guest,
                    // tsc_after_guest,
                    // vm->tsc_scaling_ratio,
                    // vm->tsc_offset,
                    // deadline.deadline);

            /* Logs */
            if (logs_buffer.size < logs_buffer.used) {
                pr_err("tansiv-timer: Buffer is full\n");
            }
            sprintf(buffer, "register-deadline;%d;%d;%llu;%llu;%llu\n",
                            deadline.pid,
                            cpu,
                            vm->deadline,
                            tsc_before_guest,
                            tsc_after_guest);
            spin_lock_irq(&logs_buffer_lock);
            cb_push(&logs_buffer, buffer);
            spin_unlock_irq(&logs_buffer_lock);
            schedule_work(&logs_work);

            if (copy_to_user(tmp, &deadline, sizeof(struct tansiv_deadline_ioctl))) {
                return -EFAULT;
            }

            break;
        }
        case TANSIV_REGISTER_HANDLER: {
            struct tansiv_vm *vm;
            struct tansiv_handler_ioctl __user *tmp = (struct tansiv_handler_ioctl *)ioctl_param;
            struct tansiv_handler_ioctl handler;
            int error;

            if (copy_from_user(&handler, tmp, sizeof(struct tansiv_handler_ioctl))) {
                return -EFAULT;
            }

            vm = find_tansiv_vm_array(&tansiv_vm_array, handler.pid);
            if (vm == NULL) {
                pr_err("tansiv-timer: TANSIV_REGISTER_HANDLER: vm %d not found\n", handler.pid);
                return -EINVAL;
            }
            

            error = register_target_pid(handler.pid, &vm->handler_pid);
            if (error) {
                unregister_target_pid(&vm->handler_pid);
                pr_err("tansiv-timer: Error while registering target pid in TANSIV_REGISTER_HANDLER of vm %d\n", handler.pid);
                return -EINVAL;
            }


            break;
        }
        case TANSIV_REGISTER_VCPU: {
            struct tansiv_vm *vm;
            struct tansiv_vcpu_ioctl __user *tmp = (struct tansiv_vcpu_ioctl *)ioctl_param;
            struct tansiv_vcpu_ioctl vcpu;

            if (copy_from_user(&vcpu, tmp, sizeof(struct tansiv_vcpu_ioctl))) {
                return -EFAULT;
            }

            pr_info("TANSIV_REGISTER_VCPU: pid = %d, vcpu pid = %d\n", vcpu.pid, vcpu.vcpu_pid);
            vm = find_tansiv_vm_array(&tansiv_vm_array, vcpu.pid);
            if (vm == NULL) {
                pr_err("tansiv-timer: TANSIV_REGISTER_VCPU: vm %d not found\n", vcpu.pid);
                return -EINVAL;
            }

            insert_struct_pid_array(&vm->vcpus_pids, vcpu.vcpu_pid);
            pr_info("TANSIV_REGISTER_VCPU: success");
            break;
        }
        case TANSIV_INIT_END: {
            struct tansiv_vm *vm;
            struct tansiv_init_end_ioctl __user *tmp = (struct tansiv_init_end_ioctl *)ioctl_param;
            struct tansiv_init_end_ioctl init_end;

            if (copy_from_user(&init_end, tmp, sizeof(struct tansiv_init_end_ioctl))) {
                return -EFAULT;
            }

            vm = find_tansiv_vm_array(&tansiv_vm_array, init_end.pid);
            if (vm == NULL) {
                pr_err("tansiv-timer: TANSIV_INIT_END: vm %d not found\n", init_end.pid);
                return -EINVAL;
            }
            vm->init_status = true;
            break;
        }
        case TANSIV_INIT_CHECK: {
            struct tansiv_vm *vm;
            struct tansiv_init_check_ioctl __user *tmp = (struct tansiv_init_check_ioctl *)ioctl_param;
            struct tansiv_init_check_ioctl init_check;

            if (copy_from_user(&init_check, tmp, sizeof(struct tansiv_init_check_ioctl))) {
                return -EFAULT;
            }
            pr_info("TANSIV_INIT_CHECK: pid = %d\n", init_check.pid);

            vm = find_tansiv_vm_array(&tansiv_vm_array, init_check.pid);
            if (vm == NULL) {
                pr_err("tansiv-timer: TANSIV_INIT_CHECK: vm %d not found\n", init_check.pid);
                return -EINVAL;
            }
            init_check.status = vm->init_status;
            if (copy_to_user(tmp, &init_check, sizeof(struct tansiv_init_check_ioctl))) {
                return -EFAULT;
            }
            break;
        }
        case TANSIV_SCALE_TSC: {
            struct tansiv_vm *vm;
            struct tansiv_scale_tsc_ioctl  __user *tmp = (struct tansiv_scale_tsc_ioctl *)ioctl_param;
            struct tansiv_scale_tsc_ioctl  scale_tsc;

            if (copy_from_user(&scale_tsc, tmp, sizeof(struct tansiv_scale_tsc_ioctl))) {
                return -EFAULT;
            }

            vm = find_tansiv_vm_array(&tansiv_vm_array, scale_tsc.pid);
             if (vm == NULL) {
                pr_err("tansiv-timer: TANSIV_SCALE_TSC: vm %d not found\n", scale_tsc.pid);
                return -EINVAL;
            }

            vm->tsc_offset = kvm_get_tsc_offset(pid_nr(&vm->pid));
            vm->tsc_scaling_ratio = kvm_get_tsc_scaling_ratio(pid_nr(&vm->pid));
            scale_tsc.scaled_tsc = kvm_scale_tsc(scale_tsc.tsc, vm->tsc_scaling_ratio) + vm->tsc_offset;
            if (copy_to_user(tmp, &scale_tsc, sizeof(struct tansiv_scale_tsc_ioctl))) {
                return -EFAULT;
            }
            break;

        }
        default:
            pr_err("tansiv-timer: Unknown ioctl command\n");
            return -EINVAL;
    }
    return 0;
}

/* File operations */
static struct file_operations fops = {
    .read = NULL,
    .write = NULL,
    .unlocked_ioctl = device_ioctl,
    .open = device_open,
    .release = device_release,
};

/* Show tsc frequency in sysfs */
// static ssize_t tsc_khz_show(struct kobject *kobj,
			    // struct kobj_attribute *attr, char *buf)
// {
	// return sprintf(buf, "%u\n", tsc_khz);
// }
// 
// struct kobj_attribute tsc_khz_attr = __ATTR_RO(tsc_khz);

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

    error = init_tansiv_vm_array(&tansiv_vm_array, DEFAULT_NUMBER_VMS);
    
    if (error < 0) {
        pr_err("tansiv-timer: failed to initialize tansiv_vm_array\n");
        return error;
    }

    /* Logs */
    s = kasprintf(GFP_KERNEL, "/tmp/tansiv_kernel.csv");
    logs_file = filp_open(s, O_CREAT | O_WRONLY | O_APPEND, 0644);
    
    pr_info("tansiv-timer: starting circular buffer initialization\n");
    cb_init(&logs_buffer, LOGS_BUFFER_SIZE, sizeof(char[LOGS_LINE_SIZE]));

    spin_lock_init(&logs_buffer_lock);
    INIT_WORK(&logs_work, write_logs);

    /* Sysfs */
    // error = sysfs_create_file(&cpu_subsys.dev_root->kobj, &tsc_khz_attr.attr);
	// if (error < 0) {
        // pr_err("tansiv-timer: failed to initialize sysfs\n");
        // return error;
    // }

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
    /* Array cleaning */
    free_tansiv_vm_array(&tansiv_vm_array);
    /* Circular buffer */
    cb_free(&logs_buffer);
    /* Sysfs */
    // sysfs_remove_file(&cpu_subsys.dev_root->kobj, &tsc_khz_attr.attr);
    pr_info("tansiv-timer: Exit success\n");
}

module_init(tansiv_timer_init);
module_exit(cancel_tansiv_timer);

MODULE_LICENSE("GPL");
MODULE_AUTHOR("Léo Cosseron");
MODULE_DESCRIPTION("Timers for TANSIV");
MODULE_VERSION("0.1");