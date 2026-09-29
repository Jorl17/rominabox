/* The unpacking dialog of the Windows game on its own, to look at while we
 * design it.
 *
 *   unpack_dialog_demo LOGO.png FONT.ttf TITLE                  shown, filling over 4 s
 *   unpack_dialog_demo LOGO.png FONT.ttf TITLE OUT.png DPI DONE  drawn into OUT.png
 */
#include "../../desktop/src-tauri/launcher/windows/unpack_dialog.h"

#include <windows.h>
#include <stdio.h>
#include <stdlib.h>

static void *slurp(const wchar_t *path, size_t *size) {
    FILE *file = _wfopen(path, L"rb");
    void *bytes;
    long length;
    if (!file)
        return NULL;
    fseek(file, 0, SEEK_END);
    length = ftell(file);
    fseek(file, 0, SEEK_SET);
    bytes = malloc((size_t)length);
    if (bytes && fread(bytes, 1, (size_t)length, file) != (size_t)length) {
        free(bytes);
        bytes = NULL;
    }
    fclose(file);
    *size = (size_t)length;
    return bytes;
}

int wmain(int argc, wchar_t **argv) {
    UnpackLook look = {0};
    if (argc != 4 && argc != 7) {
        fwprintf(stderr, L"usage: unpack_dialog_demo LOGO.png FONT.ttf TITLE [OUT.png DPI DONE]\n");
        return 2;
    }
    SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    look.title = argv[3];
    look.logo_png = slurp(argv[1], &look.logo_size);
    look.font_ttf = slurp(argv[2], &look.font_size);
    if (!look.logo_png || !look.font_ttf) {
        fwprintf(stderr, L"could not read the logo or the font\n");
        return 1;
    }
    if (argc == 7)
        return unpack_dialog_picture(&look, _wtof(argv[6]), (unsigned)_wtoi(argv[5]), argv[4]) == 0 ? 0 : 1;
    {
        UnpackDialog *dialog = unpack_dialog_open(&look);
        ULONGLONG start = GetTickCount64();
        double done = 0;
        if (!dialog)
            return 1;
        while (done < 1) {
            done = (GetTickCount64() - start) / 4000.0;
            unpack_dialog_progress(dialog, done);
            Sleep(8);
        }
        Sleep(250);
        unpack_dialog_close(dialog);
    }
    return 0;
}
