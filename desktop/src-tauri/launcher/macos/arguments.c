#include <mach-o/dyld.h>
#include <mach-o/loader.h>
#include <stdint.h>
#include <string.h>

#include "arguments.h"

/* The trampoline's tail: its magic 20 bytes after the entry, then the
 * addresses of the argc and argv slots. The same on every processor. */
void rominabox_publish_arguments(int argc, char **argv) {
    const struct mach_header_64 *header =
        (const struct mach_header_64 *)_dyld_get_image_header(0);
    const uint8_t *commands;
    uint32_t offset = 0;
    uint32_t command_index;
    if (!header || header->magic != MH_MAGIC_64)
        return;
    commands = (const uint8_t *)(header + 1);
    for (command_index = 0; command_index < header->ncmds; command_index++) {
        const struct load_command *command =
            (const struct load_command *)(commands + offset);
        if (command->cmd == LC_MAIN) {
            const struct entry_point_command *entry =
                (const struct entry_point_command *)command;
            const uint8_t *trampoline = (const uint8_t *)header + entry->entryoff;
            uint64_t argc_address;
            uint64_t argv_address;
            intptr_t slide;
            if (memcmp(trampoline + 20, "RBOXLNCH", 8) != 0)
                return;
            memcpy(&argc_address, trampoline + 28, sizeof argc_address);
            memcpy(&argv_address, trampoline + 36, sizeof argv_address);
            slide = _dyld_get_image_vmaddr_slide(0);
            *(uint64_t *)(slide + (intptr_t)argc_address) = (uint64_t)argc;
            *(uint64_t *)(slide + (intptr_t)argv_address) = (uint64_t)(uintptr_t)argv;
            return;
        }
        offset += command->cmdsize;
    }
}
