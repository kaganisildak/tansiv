#! /bin/bash

set -e

# Help
usage() {
    echo "$0 <socket_path> <vcpu_cpus_list> <main_loop_cpu> <other_threads_cpus>"
    echo "socket_path: Path to the QEMU monitor socket"
    echo "vcpus_cpus_list: Comma-separated list of CPUs to use for pinning vCPUs"
    echo "main_loop_cpu: Single CPU number to use for pinning the main loop thread"
    echo "other_threads_cpus: CPU range on which to pin the remaining QEMU threads"
    exit 1
}

# Maybe simplify and pin the remaining threads on the main loop CPU?

if [ $# -ne 4 ]; then
    echo "Insufficient arguments provided"
    usage
fi

# Get parameters (list of socket names)
socket_path=$1
vcpu_cpus_list=($(echo $2 | tr ',' ' '))
main_loop_cpu=$3
remaining_threads_cpus=$4

parse_qemu_monitor() {
    local socket_path=$1
    local command="info cpus"

    output=$(echo "$command" | socat - UNIX-CONNECT:"$socket_path")

    # echo "Raw output from $socket_path:"
    # echo "$output"
    
    echo "Extract thread id from $socket_path:"
    thread_ids=($(echo "$output" | grep -Eo "thread_id=[0-9]+" | grep -Eo "[0-9]+"))
    echo "Extracted thread IDs from $socket_name: ${thread_ids[@]}"

    # Create a cgroup for the VM
    echo "Creating a cgroup for the VM"
    cgroup_name="tansiv_$(basename $socket_path)"
    cgroup_path="/sys/fs/cgroup/$cgroup_name"

    mkdir -p "$cgroup_path"

    echo "+cpu +cpuset" | tee "$cgroup_path/cgroup.subtree_control"
    echo "threaded" | tee "$cgroup_path/cgroup.type"

    # Now, let's move the VM process in the cgroup
    # The qemu_pid should be the second process with the socket_path
    # The first one is the boot.py wrapper
    qemu_pid=$(pgrep -f "$socket_path" | sed -n '2 p')

    if [ -n "$qemu_pid" ]; then
    
        echo "Moving QEMU process $qemu_pid in the cgroup"
        echo "$qemu_pid" | tee "$cgroup_path/cgroup.procs"

        echo "Pinning QEMU main loop thread $qemu_pid to CPU $main_loop_cpu"

        # Create a sub-cgroup for this thread
        echo "Creating a sub-cgroup for thread $qemu_pid"
        thread_cgroup="$cgroup_path/thread_$qemu_pid"
        sudo mkdir -p "$thread_cgroup"

        echo "threaded" | tee "$thread_cgroup/cgroup.type"
        echo "$main_loop_cpu" | tee "$thread_cgroup/cpuset.cpus"
        echo "$qemu_pid" | tee "$thread_cgroup/cgroup.threads"

    else
        echo "Failed to find QEMU process ID"
    fi

    # Now, let's move each vCPU thread on a dedicated CPU
    for i in "${!thread_ids[@]}"; do
        thread_id=${thread_ids[$i]}
        cpu_id=${vcpu_cpus_list[$i]}

        # TODO: Don't rely on nproc for this step
        # if [ "$cpu_id" -gt $(( $(nproc) - 1 )) ]; then
            # echo "Invalid CPU Number"
            # exit 1
        # fi

        echo "Pinning thread ID $thread_id to CPU $cpu_id"

        # Create a sub-cgroup for this thread
        echo "Creating a sub-cgroup for thread $thread_id"
        thread_cgroup="$cgroup_path/thread_$thread_id"
        sudo mkdir -p "$thread_cgroup"

        echo "threaded" | tee "$thread_cgroup/cgroup.type"

        echo "$cpu_id" | tee "$thread_cgroup/cpuset.cpus"

        echo "$thread_id" | tee "$thread_cgroup/cgroup.threads"
    done

    # Now let's move the remaining QEMU threads on the specified CPUs
    # Get the remaining threads
    remaining_threads=$(cat "$cgroup_path/cgroup.threads" | tr ' ' '\n' | wc -l)
    echo "Pinning $remaining_threads remaining threads to CPU range $remaining_threads_cpus"

    # Create a sub-cgroup for the remaining threads
    echo "Creating a sub-cgroup for $remaining_threads remaining_threads"
    remaining_threads_cgroup="$cgroup_path/remaining_threads"
    sudo mkdir -p "$remaining_threads_cgroup"

    echo "threaded" | tee "$remaining_threads_cgroup/cgroup.type"

    echo "$remaining_threads_cpus" | tee "$remaining_threads_cgroup/cpuset.cpus"
    
    for thread_id in $(cat "$cgroup_path/cgroup.threads"); do echo "$thread_id" | tee -a "$remaining_threads_cgroup/cgroup.threads"; done
}

parse_qemu_monitor "$socket_path"
echo "Done."