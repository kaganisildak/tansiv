#include "vsg.h"
#include <stdio.h>
#include <stdlib.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <unistd.h>


int vsg_connect(void) {
    // TODO(msimonin): use DEBUG_CALL
    printf("VSG] CONNECT_TO_SVG\n");
    int vm_socket = socket(PF_LOCAL, SOCK_STREAM, 0);

    struct sockaddr_un address;
    address.sun_family = AF_LOCAL;
    strcpy(address.sun_path, CONNECTION_SOCKET_NAME);

    if (connect(vm_socket, (struct sockaddr*)(&address), sizeof(address)) != 0) {
        printf("VSG] ERROR CONNECTING TO %s\n", CONNECTION_SOCKET_NAME);
        return -1;
    }
    printf("VSG] CONNECTION SUCCESSFUL\n");
    return vm_socket;
}

int vsg_send(int fd, char* message, int length) {
    // TODO(msimonin) handle time
    struct vsg_send_packet packet = {{0}, {length}};
    enum vsg_msg_to_actor_type send_packet_flag = VSG_SEND_PACKET;
    int ret = 0;

    ret = send(fd, &send_packet_flag, sizeof(send_packet_flag), 0);
    if (ret < 0)
        return -1;

    ret = send(fd, &packet, sizeof(packet), 0);
    if (ret < 0)
        return -1;

    // send(vm_socket, dest.c_str(), dest.length(), 0);
    ret = send(fd, message, length, 0);
    if (ret < 0)
        return -1;
    return 0;
}