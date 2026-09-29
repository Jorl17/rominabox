/* The small window that we show while we unpack a Windows game on its first
 * launch, with the ROM-in-a-Box logo tilted as in the builder, the game's
 * title and a progress bar, in the builder's colours. We use only native
 * Win32 and GDI+. */
#ifndef ROMINABOX_UNPACK_DIALOG_H
#define ROMINABOX_UNPACK_DIALOG_H

#include <stddef.h>
#include <wchar.h>

typedef struct UnpackDialog UnpackDialog;

/* What we draw the dialog from: the logo as PNG bytes and the lettering as
 * TrueType bytes, both inside the game. */
typedef struct {
    const wchar_t *title;
    const void *logo_png;
    size_t logo_size;
    const void *font_ttf;
    size_t font_size;
} UnpackLook;

/* Show the dialog, centred on the screen under the pointer. NULL when we
 * cannot show it, and then we unpack without it. */
UnpackDialog *unpack_dialog_open(const UnpackLook *look);
/* The progress of the unpacking, 0 to 1. Also keeps the window responsive. */
void unpack_dialog_progress(UnpackDialog *dialog, double done);
void unpack_dialog_close(UnpackDialog *dialog);

/* The dialog as a PNG at `dpi`, `done` of the way, for a person to look at
 * without a window. Returns 0 when written. */
int unpack_dialog_picture(const UnpackLook *look, double done, unsigned dpi, const wchar_t *png);

#endif
