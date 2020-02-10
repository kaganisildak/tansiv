#include "vsg.h"
#include <stdio.h>

/*
 *
 * Mimic a sink
 *
 *
 */
int main(int argc, char *argv[]) {
    int vsg_socket = vsg_connect();
    uint32_t order;
    vsg_recv_order(vsg_socket, &order);
}