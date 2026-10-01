#pragma once

#if !defined(__APPLE__) && !defined(__unix__)
#error "posix/sandbox_attempts.h is what the sandbox probe tries that each platform does its own way on macOS and Linux; sandbox_probe.c names each platform's"
#endif

static void print_homes(void) {
    const char *home = getenv("HOME");
    const char *tmp = getenv("TMPDIR");
    printf("HOME=%s\n", home ? home : "");
    printf("TMPDIR=%s\n", tmp ? tmp : "");
}

static void shared_memory_attempt(void) {
    int shared = shm_open("/rominabox-isolation-probe", O_CREAT | O_RDWR, 0600);
    if (shared < 0) {
        printf("SHM_DENIED\n");
    } else {
        printf("SHM_ALLOWED\n");
        close(shared);
        shm_unlink("/rominabox-isolation-probe");
    }
}

/* The network command port. Binding it fails in the macOS sandbox. */
static void network_command_attempt(void) {
    int udp = socket(AF_INET, SOCK_DGRAM, 0);
    struct sockaddr_in address;
    memset(&address, 0, sizeof address);
    address.sin_family = AF_INET;
    address.sin_port = htons(55355);
    address.sin_addr.s_addr = htonl(INADDR_ANY);
    if (udp < 0 || bind(udp, (struct sockaddr *)&address, sizeof address) != 0)
        printf("UDP_DENIED\n");
    else
        printf("UDP_ALLOWED\n");
    if (udp >= 0)
        close(udp);
}
