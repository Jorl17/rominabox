/* A virtual Xbox 360 pad on the ViGEm bus driver, for use in tests. From a
 * test we plug the pad in, press buttons, read the rumble that a game
 * requests, and unplug the pad. To every program on Windows the pad is a
 * wired Xbox 360 controller, read through XInput like a physical one.
 *
 * Commands, one a line on standard input, each answered with one line on
 * standard output:
 *   plug              plugs the pad in: "plugged SLOT", SLOT from XInput
 *   hold INPUT...     keeps exactly these inputs down, none for none: "held"
 *   rumble MS         waits up to MS milliseconds for a game to request
 *                     rumble: "rumble LARGE SMALL" (0-255), or "rumble none"
 *   unplug            "unplugged"
 * INPUT is a button, a trigger or a stick direction, by the names below. We
 * never leave a pad behind: at the end of standard input, or when this
 * program ends, the bus removes the pad that its handle plugged in.
 *
 * Without the bus driver, every command gets the answer "no-bus". The
 * driver's interface (its device GUID, control codes and structures) is
 * the ViGEmClient library's include/ViGEm/km/BusShared.h and Common.h
 * (MIT, Nefarius Software Solutions e.U.). */
#include <windows.h>
#include <setupapi.h>
#include <initguid.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

DEFINE_GUID(GUID_DEVINTERFACE_BUSENUM_VIGEM,
    0x96E42B22, 0xF5E9, 0x42F8, 0xB0, 0x43, 0xED, 0x0F, 0x93, 0x2F, 0x01, 0x4F);

#define VIGEM_COMMON_VERSION 0x0001
#define BUS_W(index) CTL_CODE(FILE_DEVICE_BUS_EXTENDER, index, METHOD_BUFFERED, FILE_WRITE_DATA)
#define BUS_RW(index) CTL_CODE(FILE_DEVICE_BUS_EXTENDER, index, METHOD_BUFFERED, FILE_WRITE_DATA | FILE_READ_DATA)
#define IOCTL_VIGEM_PLUGIN_TARGET BUS_W(0x801)
#define IOCTL_VIGEM_UNPLUG_TARGET BUS_W(0x802)
#define IOCTL_VIGEM_CHECK_VERSION BUS_W(0x803)
#define IOCTL_VIGEM_WAIT_DEVICE_READY BUS_W(0x804)
#define IOCTL_XUSB_REQUEST_NOTIFICATION BUS_RW(0xA01)
#define IOCTL_XUSB_SUBMIT_REPORT BUS_W(0xA02)
#define IOCTL_XUSB_GET_USER_INDEX BUS_RW(0xA07)

/* The bus's first serial number; this program plugs in one pad. */
#define SERIAL 1
/* Xbox360Wired of VIGEM_TARGET_TYPE, as reported by the wired Microsoft pad. */
#define XBOX_360_WIRED 0
#define XBOX_360_VENDOR 0x045E
#define XBOX_360_PRODUCT 0x028E

typedef struct { ULONG Size, SerialNo; int TargetType; USHORT VendorId, ProductId; } PluginTarget;
typedef struct { ULONG Size, SerialNo; } TargetSerial;
typedef struct { ULONG Size, Version; } CheckVersion;
typedef struct { USHORT wButtons; BYTE bLeftTrigger, bRightTrigger; SHORT sThumbLX, sThumbLY, sThumbRX, sThumbRY; } XusbReport;
typedef struct { ULONG Size, SerialNo; XusbReport Report; } XusbSubmitReport;
typedef struct { ULONG Size, SerialNo; UCHAR LargeMotor, SmallMotor, LedNumber; } XusbNotification;
typedef struct { ULONG Size, SerialNo, UserIndex; } XusbUserIndex;

/* The buttons, by their names in a test, with their values from XUSB_BUTTON. */
static const struct { const char *name; USHORT bit; } BUTTONS[] = {
    {"up", 0x0001}, {"down", 0x0002}, {"left", 0x0004}, {"right", 0x0008},
    {"start", 0x0010}, {"back", 0x0020}, {"left-thumb", 0x0040}, {"right-thumb", 0x0080},
    {"left-shoulder", 0x0100}, {"right-shoulder", 0x0200}, {"guide", 0x0400},
    {"a", 0x1000}, {"b", 0x2000}, {"x", 0x4000}, {"y", 0x8000},
};

static HANDLE bus = INVALID_HANDLE_VALUE;
static int plugged;

/* Send one control request and wait, since the bus completes it later. */
static BOOL request(DWORD code, void *in, DWORD in_size, void *out, DWORD out_size) {
    OVERLAPPED overlapped = {0};
    DWORD returned = 0;
    BOOL done;
    overlapped.hEvent = CreateEventW(NULL, TRUE, FALSE, NULL);
    done = DeviceIoControl(bus, code, in, in_size, out, out_size, &returned, &overlapped);
    if (!done && GetLastError() == ERROR_IO_PENDING)
        done = GetOverlappedResult(bus, &overlapped, &returned, TRUE);
    CloseHandle(overlapped.hEvent);
    return done;
}

/* Open the bus device as in ViGEmClient, when the driver is installed and
 * supports the version of this interface. */
static HANDLE open_bus(void) {
    HDEVINFO set = SetupDiGetClassDevsW(&GUID_DEVINTERFACE_BUSENUM_VIGEM, NULL, NULL,
                                        DIGCF_PRESENT | DIGCF_DEVICEINTERFACE);
    SP_DEVICE_INTERFACE_DATA data;
    HANDLE opened = INVALID_HANDLE_VALUE;
    DWORD index;
    if (set == INVALID_HANDLE_VALUE)
        return opened;
    ZeroMemory(&data, sizeof data);
    data.cbSize = sizeof data;
    for (index = 0; opened == INVALID_HANDLE_VALUE
                    && SetupDiEnumDeviceInterfaces(set, NULL, &GUID_DEVINTERFACE_BUSENUM_VIGEM, index, &data); index++) {
        DWORD size = 0;
        SP_DEVICE_INTERFACE_DETAIL_DATA_W *detail;
        SetupDiGetDeviceInterfaceDetailW(set, &data, NULL, 0, &size, NULL);
        detail = malloc(size);
        if (!detail)
            break;
        detail->cbSize = sizeof *detail;
        if (SetupDiGetDeviceInterfaceDetailW(set, &data, detail, size, NULL, NULL)) {
            opened = CreateFileW(detail->DevicePath, GENERIC_READ | GENERIC_WRITE, FILE_SHARE_READ | FILE_SHARE_WRITE,
                                 NULL, OPEN_EXISTING,
                                 FILE_ATTRIBUTE_NORMAL | FILE_FLAG_NO_BUFFERING | FILE_FLAG_WRITE_THROUGH | FILE_FLAG_OVERLAPPED,
                                 NULL);
            if (opened != INVALID_HANDLE_VALUE) {
                CheckVersion version = {sizeof version, VIGEM_COMMON_VERSION};
                HANDLE previous = bus;
                bus = opened;
                if (!request(IOCTL_VIGEM_CHECK_VERSION, &version, sizeof version, NULL, 0)) {
                    CloseHandle(opened);
                    opened = INVALID_HANDLE_VALUE;
                }
                bus = previous;
            }
        }
        free(detail);
    }
    SetupDiDestroyDeviceInfoList(set);
    return opened;
}

static void plug(void) {
    PluginTarget target = {sizeof target, SERIAL, XBOX_360_WIRED, XBOX_360_VENDOR, XBOX_360_PRODUCT};
    TargetSerial ready = {sizeof ready, SERIAL};
    XusbUserIndex slot = {sizeof slot, SERIAL, 0};
    if (plugged || !request(IOCTL_VIGEM_PLUGIN_TARGET, &target, sizeof target, NULL, 0)
        || !request(IOCTL_VIGEM_WAIT_DEVICE_READY, &ready, sizeof ready, NULL, 0)) {
        printf("failed plug %lu\n", GetLastError());
        return;
    }
    plugged = 1;
    /* The pad has an XInput slot once XInput has enumerated it. */
    for (int tries = 0; tries < 50 && !request(IOCTL_XUSB_GET_USER_INDEX, &slot, sizeof slot, &slot, sizeof slot); tries++)
        Sleep(100);
    printf("plugged %lu\n", slot.UserIndex);
}

/* The triggers pulled all the way and each stick moved all the way, by
 * name: the part of the report for each, and its value. */
enum Part { LEFT_TRIGGER, RIGHT_TRIGGER, LEFT_X, LEFT_Y, RIGHT_X, RIGHT_Y };
static const struct { const char *name; enum Part part; int value; } AXES[] = {
    {"left-trigger", LEFT_TRIGGER, 255}, {"right-trigger", RIGHT_TRIGGER, 255},
    {"left-stick-left", LEFT_X, -32768}, {"left-stick-right", LEFT_X, 32767},
    {"left-stick-down", LEFT_Y, -32768}, {"left-stick-up", LEFT_Y, 32767},
    {"right-stick-left", RIGHT_X, -32768}, {"right-stick-right", RIGHT_X, 32767},
    {"right-stick-down", RIGHT_Y, -32768}, {"right-stick-up", RIGHT_Y, 32767},
};

static int set(XusbReport *report, const char *name) {
    for (size_t index = 0; index < sizeof BUTTONS / sizeof BUTTONS[0]; index++)
        if (strcmp(BUTTONS[index].name, name) == 0) {
            report->wButtons |= BUTTONS[index].bit;
            return 1;
        }
    for (size_t index = 0; index < sizeof AXES / sizeof AXES[0]; index++)
        if (strcmp(AXES[index].name, name) == 0) {
            int value = AXES[index].value;
            switch (AXES[index].part) {
            case LEFT_TRIGGER: report->bLeftTrigger = (BYTE)value; break;
            case RIGHT_TRIGGER: report->bRightTrigger = (BYTE)value; break;
            case LEFT_X: report->sThumbLX = (SHORT)value; break;
            case LEFT_Y: report->sThumbLY = (SHORT)value; break;
            case RIGHT_X: report->sThumbRX = (SHORT)value; break;
            case RIGHT_Y: report->sThumbRY = (SHORT)value; break;
            }
            return 1;
        }
    return 0;
}

static void hold(char *names) {
    XusbSubmitReport report = {sizeof report, SERIAL, {0}};
    for (char *name = strtok(names, " \t"); name; name = strtok(NULL, " \t"))
        if (!set(&report.Report, name)) {
            printf("failed no-input %s\n", name);
            return;
        }
    printf(plugged && request(IOCTL_XUSB_SUBMIT_REPORT, &report, sizeof report, NULL, 0) ? "held\n" : "failed hold\n");
}

/* The bus answers a notification request each time a program sets the
 * motors or light of the pad. The first answer can be the previous state. */
static void rumble(DWORD milliseconds) {
    DWORD deadline = GetTickCount() + milliseconds;
    while (plugged && (LONG)(deadline - GetTickCount()) > 0) {
        XusbNotification asked = {sizeof asked, SERIAL, 0, 0, 0};
        OVERLAPPED overlapped = {0};
        DWORD returned = 0;
        overlapped.hEvent = CreateEventW(NULL, TRUE, FALSE, NULL);
        if (!DeviceIoControl(bus, IOCTL_XUSB_REQUEST_NOTIFICATION, &asked, sizeof asked, &asked, sizeof asked,
                             &returned, &overlapped) && GetLastError() != ERROR_IO_PENDING) {
            CloseHandle(overlapped.hEvent);
            break;
        }
        DWORD left = deadline - GetTickCount();
        if (WaitForSingleObject(overlapped.hEvent, (LONG)left > 0 ? left : 0) != WAIT_OBJECT_0) {
            CancelIoEx(bus, &overlapped);
            GetOverlappedResult(bus, &overlapped, &returned, TRUE);
            CloseHandle(overlapped.hEvent);
            break;
        }
        GetOverlappedResult(bus, &overlapped, &returned, FALSE);
        CloseHandle(overlapped.hEvent);
        if (asked.LargeMotor || asked.SmallMotor) {
            printf("rumble %u %u\n", asked.LargeMotor, asked.SmallMotor);
            return;
        }
    }
    printf("rumble none\n");
}

static void unplug(void) {
    TargetSerial target = {sizeof target, SERIAL};
    if (plugged && request(IOCTL_VIGEM_UNPLUG_TARGET, &target, sizeof target, NULL, 0))
        plugged = 0;
    printf(plugged ? "failed unplug\n" : "unplugged\n");
}

int main(void) {
    char line[512];
    setvbuf(stdout, NULL, _IONBF, 0);
    bus = open_bus();
    while (fgets(line, sizeof line, stdin)) {
        char *end = line + strcspn(line, "\r\n");
        char *argument;
        *end = '\0';
        argument = line + strcspn(line, " ");
        if (*argument)
            *argument++ = '\0';
        if (bus == INVALID_HANDLE_VALUE)
            printf("no-bus\n");
        else if (strcmp(line, "plug") == 0)
            plug();
        else if (strcmp(line, "hold") == 0)
            hold(argument);
        else if (strcmp(line, "rumble") == 0)
            rumble((DWORD)strtoul(argument, NULL, 10));
        else if (strcmp(line, "unplug") == 0)
            unplug();
        else
            printf("failed no-command %s\n", line);
    }
    if (plugged) {
        TargetSerial target = {sizeof target, SERIAL};
        request(IOCTL_VIGEM_UNPLUG_TARGET, &target, sizeof target, NULL, 0);
    }
    if (bus != INVALID_HANDLE_VALUE)
        CloseHandle(bus);
    return 0;
}
