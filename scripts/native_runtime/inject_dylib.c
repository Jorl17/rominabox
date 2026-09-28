#include <errno.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <mach-o/loader.h>

/* Attach a launch library to one slice of a Mac player, which is a thin
 * arm64 or x86_64 executable. Add the library to the player's load commands,
 * and at the entry point put a trampoline that calls main with the argc and
 * argv from two slots at the end of __DATA, filled in by the library. The
 * trampoline is 20 bytes of code for the slice's processor, then "RBOXLNCH"
 * and the slot addresses, read in launcher/macos/arguments.c. We split a
 * universal player into slices, attach each one and join them again. */

/* The trampoline's code, before its tail. */
#define TRAMPOLINE_CODE 20

static void fail(const char *message) {
    fprintf(stderr, "inject_dylib: %s\n", message);
    exit(1);
}

static uint32_t adrp(uint32_t rd, uint64_t pc, uint64_t target) {
    int64_t imm = (int64_t)((target & ~0xfffULL) - (pc & ~0xfffULL)) >> 12;
    uint32_t encoded = (uint32_t)imm & 0x1fffff;
    uint32_t immlo = encoded & 3;
    uint32_t immhi = (encoded >> 2) & 0x7ffff;
    return (1u << 31) | (immlo << 29) | (0x10u << 24) | (immhi << 5) | rd;
}

static uint32_t ldr_unsigned(uint32_t rt, uint32_t rn, uint32_t byte_offset) {
    uint32_t imm12 = byte_offset >> 3;
    return (0x3u << 30) | (0x7u << 27) | (0x1u << 24) | (0x1u << 22) |
        (imm12 << 10) | (rn << 5) | rt;
}

static uint32_t branch(uint64_t pc, uint64_t target) {
    int64_t imm = ((int64_t)target - (int64_t)pc) >> 2;
    if (imm < -(1 << 25) || imm >= (1 << 25))
        fail("the runtime entry is too far away to redirect");
    return (0x5u << 26) | ((uint32_t)imm & 0x3ffffff);
}

/* x0 and x1 loaded from the slots, then a branch to main. */
static void arm64_trampoline(uint8_t code[TRAMPOLINE_CODE], uint64_t at,
        uint64_t argc_slot, uint64_t argv_slot, uint64_t main_vm) {
    uint32_t words[5];
    words[0] = adrp(0, at, argc_slot);
    words[1] = ldr_unsigned(0, 0, (uint32_t)(argc_slot & 0xfff));
    words[2] = adrp(1, at + 8, argv_slot);
    words[3] = ldr_unsigned(1, 1, (uint32_t)(argv_slot & 0xfff));
    words[4] = branch(at + 16, main_vm);
    memcpy(code, words, sizeof words);
}

/* A displacement from `from`, the address after the instruction, to `to`. */
static uint32_t displacement(uint64_t from, uint64_t to) {
    int64_t distance = (int64_t)(to - from);
    if (distance < INT32_MIN || distance > INT32_MAX)
        fail("the runtime entry is too far away to redirect");
    return (uint32_t)(int32_t)distance;
}

/* edi and rsi loaded from the slots, then a jump to main. The stack is as it
 * was at the entry point, so the calling convention for main is still met. */
static void x86_64_trampoline(uint8_t code[TRAMPOLINE_CODE], uint64_t at,
        uint64_t argc_slot, uint64_t argv_slot, uint64_t main_vm) {
    uint32_t argc_from = displacement(at + 6, argc_slot);
    uint32_t argv_from = displacement(at + 13, argv_slot);
    uint32_t to_main = displacement(at + 18, main_vm);
    memset(code, 0xcc, TRAMPOLINE_CODE); /* int3 after the jump */
    code[0] = 0x8b; code[1] = 0x3d; /* mov edi, [rip + argc_from] */
    memcpy(code + 2, &argc_from, 4);
    code[6] = 0x48; code[7] = 0x8b; code[8] = 0x35; /* mov rsi, [rip + argv_from] */
    memcpy(code + 9, &argv_from, 4);
    code[13] = 0xe9; /* jmp main */
    memcpy(code + 14, &to_main, 4);
}

static void install_trampoline(FILE *file, struct mach_header_64 *header) {
    uint8_t commands[64 * 1024];
    uint32_t offset = 0;
    uint32_t index;
    uint64_t text_vm = 0;
    uint64_t text_file = 0;
    uint64_t text_end = 0;
    uint64_t text_used = 0;
    uint64_t data_vm = 0;
    uint64_t data_size = 0;
    uint64_t data_used = 0;
    int have_text = 0;
    int have_data = 0;
    uint64_t entry = 0;
    uint32_t entry_offset = 0;
    uint64_t trampoline_file;
    uint64_t trampoline_vm;
    uint64_t argc_slot;
    uint64_t argv_slot;
    uint8_t code[TRAMPOLINE_CODE];
    uint8_t tail[24];
    if (header->sizeofcmds > sizeof commands)
        fail("the runtime header is unexpectedly large");
    if (fread(commands, 1, header->sizeofcmds, file) != header->sizeofcmds)
        fail("could not read the runtime header");
    for (index = 0; index < header->ncmds; index++) {
        struct load_command *command = (struct load_command *)(commands + offset);
        if (command->cmd == LC_SEGMENT_64) {
            struct segment_command_64 *segment = (struct segment_command_64 *)command;
            struct section_64 *section = (struct section_64 *)(segment + 1);
            uint32_t section_index;
            uint64_t used = 0;
            for (section_index = 0; section_index < segment->nsects; section_index++) {
                uint64_t end = section[section_index].addr + section[section_index].size;
                if (end > used)
                    used = end;
            }
            if (strcmp(segment->segname, "__TEXT") == 0) {
                text_vm = segment->vmaddr;
                text_file = segment->fileoff;
                text_end = segment->fileoff + segment->filesize;
                text_used = segment->fileoff;
                for (section_index = 0; section_index < segment->nsects; section_index++) {
                    uint64_t end = section[section_index].offset + section[section_index].size;
                    if (section[section_index].offset != 0 && end > text_used)
                        text_used = end;
                }
                have_text = 1;
            }
            if (strcmp(segment->segname, "__DATA") == 0) {
                data_vm = segment->vmaddr;
                data_size = segment->vmsize;
                data_used = used;
                have_data = 1;
            }
        }
        if (command->cmd == LC_MAIN)
            entry_offset = offset;
        offset += command->cmdsize;
    }
    if (!have_text || !have_data || entry_offset == 0)
        return;
    entry = ((struct entry_point_command *)(commands + entry_offset))->entryoff;
    if (fseek(file, (long)entry, SEEK_SET) != 0)
        fail("could not read the runtime entry");
    if (fread(tail, 1, 8, file) == 8) {
        /* Read the magic again, 20 bytes after a trampoline we already wrote. */
    }
    if (fseek(file, (long)entry + TRAMPOLINE_CODE, SEEK_SET) == 0 &&
        fread(tail, 1, 8, file) == 8 && memcmp(tail, "RBOXLNCH", 8) == 0)
        return;
    trampoline_file = (text_used + 3) & ~3ULL;
    if (trampoline_file < text_file || trampoline_file + 48 > text_end)
        return;
    argc_slot = (data_vm + data_size - 16) & ~7ULL;
    argv_slot = argc_slot + 8;
    if (argc_slot < data_used)
        return;
    trampoline_vm = text_vm + (trampoline_file - text_file);
    switch (header->cputype) {
    case CPU_TYPE_ARM64:
        arm64_trampoline(code, trampoline_vm, argc_slot, argv_slot,
            text_vm + (entry - text_file));
        break;
    case CPU_TYPE_X86_64:
        x86_64_trampoline(code, trampoline_vm, argc_slot, argv_slot,
            text_vm + (entry - text_file));
        break;
    default:
        fail("the runtime is not a thin arm64 or x86_64 executable");
    }
    memset(tail, 0, sizeof tail);
    memcpy(tail, "RBOXLNCH", 8);
    memcpy(tail + 8, &argc_slot, 8);
    memcpy(tail + 16, &argv_slot, 8);
    if (fseek(file, (long)trampoline_file, SEEK_SET) != 0)
        fail("could not write the launch trampoline");
    if (fwrite(code, 1, sizeof code, file) != sizeof code ||
        fwrite(tail, 1, sizeof tail, file) != sizeof tail)
        fail("could not write the launch trampoline");
    ((struct entry_point_command *)(commands + entry_offset))->entryoff = trampoline_file;
    if (fseek(file, (long)(sizeof(*header) + entry_offset), SEEK_SET) != 0)
        fail("could not redirect the runtime entry");
    if (fwrite(commands + entry_offset, 1,
            sizeof(struct entry_point_command), file) != sizeof(struct entry_point_command))
        fail("could not redirect the runtime entry");
}

static void install_load_command(FILE *file, struct mach_header_64 *header, const char *path) {
    uint8_t commands[64 * 1024];
    uint32_t offset = 0;
    uint32_t index;
    uint32_t path_size = (uint32_t)strlen(path) + 1;
    uint32_t command_size = (24 + path_size + 7) & ~7u;
    uint64_t first_section = UINT64_MAX;
    struct dylib_command command;
    char storage[512];
    if (header->sizeofcmds > sizeof commands)
        fail("the runtime header is unexpectedly large");
    if (fseek(file, sizeof *header, SEEK_SET) != 0 ||
        fread(commands, 1, header->sizeofcmds, file) != header->sizeofcmds)
        fail("could not read the runtime header");
    for (index = 0; index < header->ncmds; index++) {
        struct load_command *load = (struct load_command *)(commands + offset);
        if (load->cmd == LC_LOAD_DYLIB || load->cmd == LC_LOAD_WEAK_DYLIB) {
            struct dylib_command *dylib = (struct dylib_command *)load;
            const char *name = (const char *)load + dylib->dylib.name.offset;
            if (strcmp(name, path) == 0)
                return;
        }
        if (load->cmd == LC_SEGMENT_64) {
            struct segment_command_64 *segment = (struct segment_command_64 *)load;
            struct section_64 *section = (struct section_64 *)(segment + 1);
            uint32_t section_index;
            for (section_index = 0; section_index < segment->nsects; section_index++) {
                if (section[section_index].offset != 0 &&
                    section[section_index].offset < first_section)
                    first_section = section[section_index].offset;
            }
        }
        offset += load->cmdsize;
    }
    if (first_section == UINT64_MAX)
        fail("the runtime has no sections");
    if (sizeof(*header) + header->sizeofcmds + command_size > first_section)
        fail("the runtime has no room for the launcher");
    if (command_size > sizeof storage)
        fail("the launcher path is too long");
    memset(&command, 0, sizeof command);
    command.cmd = LC_LOAD_DYLIB;
    command.cmdsize = command_size;
    command.dylib.name.offset = 24;
    command.dylib.compatibility_version = 0x10000;
    command.dylib.current_version = 0x10000;
    memset(storage, 0, sizeof storage);
    memcpy(storage, &command, sizeof command);
    memcpy(storage + 24, path, path_size);
    if (fseek(file, sizeof(*header) + header->sizeofcmds, SEEK_SET) != 0 ||
        fwrite(storage, 1, command_size, file) != command_size)
        fail("could not attach the launcher");
    header->ncmds += 1;
    header->sizeofcmds += command_size;
    if (fseek(file, 0, SEEK_SET) != 0 ||
        fwrite(header, 1, sizeof *header, file) != sizeof *header)
        fail("could not attach the launcher");
}

int main(int argc, char **argv) {
    FILE *file;
    struct mach_header_64 header;
    if (argc != 3)
        fail("usage: inject_dylib <mach-o> <load-path>");
    file = fopen(argv[1], "r+b");
    if (!file)
        fail(strerror(errno));
    if (fread(&header, 1, sizeof header, file) != sizeof header)
        fail("could not read the runtime header");
    if (header.magic != MH_MAGIC_64 ||
            (header.cputype != CPU_TYPE_ARM64 && header.cputype != CPU_TYPE_X86_64))
        fail("the runtime is not a thin arm64 or x86_64 executable");
    install_load_command(file, &header, argv[2]);
    if (fseek(file, 0, SEEK_SET) != 0 ||
        fread(&header, 1, sizeof header, file) != sizeof header)
        fail("could not reread the runtime header");
    install_trampoline(file, &header);
    fclose(file);
    return 0;
}
