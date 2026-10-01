#pragma once

#ifndef _WIN32
#error "windows/sandbox_attempts.h is what the sandbox probe tries that each platform does its own way on Windows; sandbox_probe.c names each platform's"
#endif

/* Inside a Windows sandbox, the per-user folders belong to the sandbox. */
static void print_homes(void) {
    const char *local = getenv("LOCALAPPDATA");
    const char *temp = getenv("TEMP");
    printf("HOME=%s\n", local ? local : "");
    printf("TMPDIR=%s\n", temp ? temp : "");
}

/* Shared memory created by the host, opened by its name in the host. A
 * sandbox has separate named objects, so the host's are out of reach. */
static void shared_memory_attempt(void) {
    const char *name = getenv("ROMINABOX_PROBE_SHM");
    HANDLE shared;
    if (!name || !name[0]) {
        printf("SHM_UNSET\n");
        return;
    }
    shared = OpenFileMappingA(FILE_MAP_READ, FALSE, name);
    printf(shared ? "SHM_ALLOWED\n" : "SHM_DENIED\n");
    if (shared)
        CloseHandle(shared);
}

/* The network command port. A program in a Windows sandbox may bind it, but
 * nothing from outside reaches it, which we check by sending datagrams. */
static void network_command_attempt(void) {
    WSADATA wsa;
    SOCKET udp;
    struct sockaddr_in address;
    DWORD wait = 4000;
    char datagram[64];
    int received = 0;
    WSAStartup(MAKEWORD(2, 2), &wsa);
    udp = socket(AF_INET, SOCK_DGRAM, 0);
    memset(&address, 0, sizeof address);
    address.sin_family = AF_INET;
    address.sin_port = htons(NETWORK_COMMAND_PORT);
    address.sin_addr.s_addr = htonl(INADDR_ANY);
    if (udp != INVALID_SOCKET && bind(udp, (struct sockaddr *)&address, sizeof address) == 0) {
        setsockopt(udp, SOL_SOCKET, SO_RCVTIMEO, (const char *)&wait, sizeof wait);
        received = recv(udp, datagram, sizeof datagram, 0) > 0;
    }
    printf(received ? "UDP_ALLOWED\n" : "UDP_DENIED\n");
    if (udp != INVALID_SOCKET)
        closesocket(udp);
    WSACleanup();
}
