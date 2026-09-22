#include <errno.h>
#include <fcntl.h>
#include <netinet/in.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/socket.h>
#include <unistd.h>

static void access_attempt(const char *env_name, const char *label, int writing) {
    const char *path = getenv(env_name);
    int fd;
    if (!path || !path[0]) {
        printf("%s_UNSET\n", label);
        return;
    }
    if (writing)
        fd = open(path, O_WRONLY | O_CREAT | O_EXCL, 0644);
    else
        fd = open(path, O_RDONLY);
    if (fd < 0) {
        printf("%s_DENIED\n", label);
        return;
    }
    printf("%s_ALLOWED\n", label);
    close(fd);
}

int rarch_main(int argc, char **argv, void *data) {
    (void)argc;
    (void)argv;
    (void)data;
    const char *home = getenv("HOME");
    const char *tmp = getenv("TMPDIR");
    int shared;
    int udp;
    struct sockaddr_in address;

    printf("HOME=%s\n", home ? home : "");
    printf("TMPDIR=%s\n", tmp ? tmp : "");
    access_attempt("ROMINABOX_PROBE_READ", "READ", 0);
    access_attempt("ROMINABOX_PROBE_WRITE", "WRITE", 1);
    access_attempt("ROMINABOX_PROBE_OTHER", "OTHER", 0);

    shared = shm_open("/rominabox-isolation-probe", O_CREAT | O_RDWR, 0600);
    if (shared < 0) {
        printf("SHM_DENIED\n");
    } else {
        printf("SHM_ALLOWED\n");
        close(shared);
        shm_unlink("/rominabox-isolation-probe");
    }

    udp = socket(AF_INET, SOCK_DGRAM, 0);
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
    return 0;
}

int main(void) {
    return rarch_main(0, NULL, NULL);
}
