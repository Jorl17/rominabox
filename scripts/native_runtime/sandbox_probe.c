/* A stand-in player for the isolation tests. We start it in the sandbox of
 * the exported game, in place of the player. For each thing that a game must
 * not do, we try it and print one word into the game's launch log. The file
 * checks are the same on every platform. The sandbox home folder, the names
 * of shared memory and the way we protect the network command port differ
 * between platforms. */
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#if defined(_WIN32)
#include <winsock2.h>
#include <windows.h>
#include <io.h>
#define probe_open _open
#define probe_close _close
#define probe_unlink _unlink
#elif defined(__APPLE__) || defined(__unix__)
#include <netinet/in.h>
#include <sys/mman.h>
#include <sys/socket.h>
#include <unistd.h>
#define probe_open open
#define probe_close close
#define probe_unlink unlink
#else
#error "the sandbox probe has no checks declared for this platform"
#endif

static void access_attempt(const char *env_name, const char *label, int writing) {
    const char *path = getenv(env_name);
    int fd;
    if (!path || !path[0]) {
        printf("%s_UNSET\n", label);
        return;
    }
    if (writing)
        fd = probe_open(path, O_WRONLY | O_CREAT | O_EXCL, 0644);
    else
        fd = probe_open(path, O_RDONLY);
    if (fd < 0) {
        printf("%s_DENIED\n", label);
        return;
    }
    printf("%s_ALLOWED\n", label);
    probe_close(fd);
}

/* The QUICK SIGN IN folder, whose path comes from the launcher, and a file
 * next to it, which must stay out of reach of the opened folder. */
static void accounts_attempt(void) {
    const char *accounts = getenv("ROMINABOX_ACCOUNTS_DIR");
    char path[4096];
    int fd;
    if (!accounts || !accounts[0]) {
        printf("ACCOUNTS_UNSET\n");
        return;
    }
    snprintf(path, sizeof path, "%s/probe-file", accounts);
    fd = probe_open(path, O_WRONLY | O_CREAT | O_TRUNC, 0600);
    printf(fd >= 0 ? "ACCOUNTS_ALLOWED\n" : "ACCOUNTS_DENIED\n");
    if (fd >= 0) {
        probe_close(fd);
        probe_unlink(path);
    }
}

#if defined(_WIN32)
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
    address.sin_port = htons(55355);
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
#elif defined(__APPLE__) || defined(__unix__)
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
#endif

int rarch_main(int argc, char **argv, void *data) {
    (void)argc;
    (void)argv;
    (void)data;
    print_homes();
    access_attempt("ROMINABOX_PROBE_READ", "READ", 0);
    access_attempt("ROMINABOX_PROBE_WRITE", "WRITE", 1);
    access_attempt("ROMINABOX_PROBE_OTHER", "OTHER", 0);
    accounts_attempt();
    access_attempt("ROMINABOX_PROBE_BESIDE", "BESIDE", 0);
    shared_memory_attempt();
    network_command_attempt();
    return 0;
}

int main(void) {
    return rarch_main(0, NULL, NULL);
}
