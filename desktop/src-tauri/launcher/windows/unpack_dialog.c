/* The unpacking dialog on first launch (unpack_dialog.h). The card looks
 * like the builder's pop-up, with white paper, a 3-pixel ink border and a
 * hard 5-pixel ink shadow. The logo is the builder's drop-zone logo, turned 7
 * degrees with its own hard shadow. We use a layered window, so the card's
 * shadow falls on whatever is behind it. */
#define COBJMACROS
#include "unpack_dialog.h"

#include <windows.h>
#include <gdiplus.h>
#include <shellscalingapi.h>
#include <shlwapi.h>
#include <stdlib.h>
#include <string.h>

/* ROM-in-a-Box's own colours from the default palette (desktop/designs.json).
 * Ink is the palette's background and the bar is its screen colour. */
#define INK 0xff041738u
#define PAPER 0xffffffffu
#define BAR 0xff1049c3u
#define INK_R 0x04
#define INK_G 0x17
#define INK_B 0x38

/* The card in pixels at 96 dots per inch. */
enum {
    CARD_WIDTH = 320,
    CARD_HEIGHT = 92,
    BORDER = 3,
    SHADOW = 5,
    LOGO = 54,
    LOGO_LEFT = 18,
    LOGO_SHADOW = 4,
    TEXT_LEFT = 90,
    TEXT_RIGHT = 22,
    TITLE_TOP = 22,
    TITLE_HEIGHT = 22,
    BAR_TOP = 52,
    BAR_HEIGHT = 12,
    BAR_BORDER = 2,
};
#define TILT (-7.0f)
#define TITLE_SIZE 15.0f

typedef struct {
    ULONG_PTR gdiplus;
    GpBitmap *logo;
    GpFontCollection *fonts;
    GpFontFamily *family;
} Look;

struct UnpackDialog {
    Look look;
    wchar_t *title;
    HWND window;
    HDC canvas_dc;
    HBITMAP canvas_bitmap;
    void *canvas_bits;
    int width;
    int height;
    float scale;
    int bar_drawn;
};

static const wchar_t window_class[] = L"RomInABoxUnpack";

static void forget(Look *look) {
    if (look->logo)
        GdipDisposeImage((GpImage *)look->logo);
    if (look->fonts)
        GdipDeletePrivateFontCollection(&look->fonts);
    if (look->gdiplus)
        GdiplusShutdown(look->gdiplus);
    memset(look, 0, sizeof *look);
}

static int prepare(const UnpackLook *given, Look *look) {
    GdiplusStartupInput input = {1, NULL, FALSE, FALSE};
    IStream *stream;
    INT found = 0;
    memset(look, 0, sizeof *look);
    if (GdiplusStartup(&look->gdiplus, &input, NULL) != Ok)
        return -1;
    stream = SHCreateMemStream(given->logo_png, (UINT)given->logo_size);
    if (!stream || GdipCreateBitmapFromStream(stream, &look->logo) != Ok) {
        if (stream)
            IStream_Release(stream);
        forget(look);
        return -1;
    }
    IStream_Release(stream);
    if (GdipNewPrivateFontCollection(&look->fonts) != Ok
        || GdipPrivateAddMemoryFont(look->fonts, given->font_ttf, (INT)given->font_size) != Ok
        || GdipGetFontCollectionFamilyList(look->fonts, 1, &look->family, &found) != Ok || found != 1) {
        forget(look);
        return -1;
    }
    return 0;
}

static void measure(float scale, int *width, int *height) {
    *width = (int)((CARD_WIDTH + SHADOW) * scale + 0.5f);
    *height = (int)((CARD_HEIGHT + SHADOW) * scale + 0.5f);
}

static void fill(GpGraphics *graphics, ARGB colour, float x, float y, float width, float height) {
    GpSolidFill *brush;
    GdipCreateSolidFill(colour, &brush);
    GdipFillRectangle(graphics, (GpBrush *)brush, x, y, width, height);
    GdipDeleteBrush((GpBrush *)brush);
}

/* Draw the logo turned about its centre, with `attributes` (the shadow's
 * colour) or as it is. */
static void logo(GpGraphics *graphics, const Look *look, float x, float y, float size, GpImageAttributes *attributes) {
    UINT width = 0;
    UINT height = 0;
    GdipGetImageWidth((GpImage *)look->logo, &width);
    GdipGetImageHeight((GpImage *)look->logo, &height);
    GdipResetWorldTransform(graphics);
    GdipTranslateWorldTransform(graphics, x + size / 2, y + size / 2, MatrixOrderPrepend);
    GdipRotateWorldTransform(graphics, TILT, MatrixOrderPrepend);
    GdipDrawImageRectRect(graphics, (GpImage *)look->logo, -size / 2, -size / 2, size, size, 0, 0, (REAL)width,
                          (REAL)height, UnitPixel, attributes, NULL, NULL);
    GdipResetWorldTransform(graphics);
}

static void draw(GpGraphics *graphics, const Look *look, const wchar_t *title, double done, float scale) {
    const float card_width = CARD_WIDTH * scale;
    const float card_height = CARD_HEIGHT * scale;
    const float border = BORDER * scale;
    const float text_left = TEXT_LEFT * scale;
    const float text_width = (CARD_WIDTH - TEXT_LEFT - TEXT_RIGHT) * scale;
    const float bar_top = BAR_TOP * scale;
    const float bar_height = BAR_HEIGHT * scale;
    const float bar_border = BAR_BORDER * scale;
    const float logo_size = LOGO * scale;
    const float logo_top = (card_height - logo_size) / 2;
    ColorMatrix silhouette = {{{0}}};
    GpImageAttributes *shadow;
    GpFont *font = NULL;
    GpStringFormat *format;
    GpSolidFill *ink;
    RectF line = {text_left, TITLE_TOP * scale, text_width, TITLE_HEIGHT * scale};
    float filled;

    GdipSetSmoothingMode(graphics, SmoothingModeAntiAlias);
    GdipSetPixelOffsetMode(graphics, PixelOffsetModeHalf);
    GdipSetInterpolationMode(graphics, InterpolationModeHighQualityBicubic);
    GdipSetCompositingQuality(graphics, CompositingQualityHighQuality);
    GdipSetTextRenderingHint(graphics, TextRenderingHintAntiAliasGridFit);
    GdipGraphicsClear(graphics, 0);

    /* The card and its shadow. */
    fill(graphics, INK, SHADOW * scale, SHADOW * scale, card_width, card_height);
    fill(graphics, INK, 0, 0, card_width, card_height);
    fill(graphics, PAPER, border, border, card_width - 2 * border, card_height - 2 * border);

    /* The logo's shadow is the logo in ink, where it is opaque. */
    silhouette.m[3][3] = 1;
    silhouette.m[4][0] = INK_R / 255.0f;
    silhouette.m[4][1] = INK_G / 255.0f;
    silhouette.m[4][2] = INK_B / 255.0f;
    silhouette.m[4][4] = 1;
    GdipCreateImageAttributes(&shadow);
    GdipSetImageAttributesColorMatrix(shadow, ColorAdjustTypeDefault, TRUE, &silhouette, NULL, ColorMatrixFlagsDefault);
    logo(graphics, look, LOGO_LEFT * scale + LOGO_SHADOW * scale, logo_top + LOGO_SHADOW * scale, logo_size, shadow);
    GdipDisposeImageAttributes(shadow);
    logo(graphics, look, LOGO_LEFT * scale, logo_top, logo_size, NULL);

    /* The title, on one line, cut with an ellipsis when it is long. */
    if (GdipCreateFont(look->family, TITLE_SIZE * scale, FontStyleBold, UnitPixel, &font) != Ok)
        GdipCreateFont(look->family, TITLE_SIZE * scale, FontStyleRegular, UnitPixel, &font);
    GdipCreateStringFormat(StringFormatFlagsNoWrap, LANG_NEUTRAL, &format);
    GdipSetStringFormatTrimming(format, StringTrimmingEllipsisCharacter);
    GdipSetStringFormatLineAlign(format, StringAlignmentCenter);
    GdipCreateSolidFill(INK, &ink);
    if (font)
        GdipDrawString(graphics, title, -1, font, &line, format, (GpBrush *)ink);
    GdipDeleteBrush((GpBrush *)ink);
    GdipDeleteStringFormat(format);
    if (font)
        GdipDeleteFont(font);

    /* The bar: an ink outline, paper inside, filled with the screen colour. */
    fill(graphics, INK, text_left, bar_top, text_width, bar_height);
    fill(graphics, PAPER, text_left + bar_border, bar_top + bar_border, text_width - 2 * bar_border,
         bar_height - 2 * bar_border);
    if (done < 0)
        done = 0;
    if (done > 1)
        done = 1;
    filled = (float)((text_width - 2 * bar_border) * done);
    if (filled > 0)
        fill(graphics, BAR, text_left + bar_border, bar_top + bar_border, filled, bar_height - 2 * bar_border);
}

static int bar_pixels(const UnpackDialog *dialog, double done) {
    return (int)((CARD_WIDTH - TEXT_LEFT - TEXT_RIGHT - 2 * BAR_BORDER) * dialog->scale * done);
}

static void present(UnpackDialog *dialog, double done) {
    GpBitmap *canvas;
    GpGraphics *graphics;
    POINT origin = {0, 0};
    SIZE size = {dialog->width, dialog->height};
    BLENDFUNCTION blend = {AC_SRC_OVER, 0, 255, AC_SRC_ALPHA};
    if (GdipCreateBitmapFromScan0(dialog->width, dialog->height, dialog->width * 4, PixelFormat32bppPARGB,
                                  dialog->canvas_bits, &canvas) != Ok)
        return;
    if (GdipGetImageGraphicsContext((GpImage *)canvas, &graphics) == Ok) {
        draw(graphics, &dialog->look, dialog->title, done, dialog->scale);
        GdipFlush(graphics, FlushIntentionSync);
        GdipDeleteGraphics(graphics);
    }
    GdipDisposeImage((GpImage *)canvas);
    UpdateLayeredWindow(dialog->window, NULL, NULL, &size, dialog->canvas_dc, &origin, 0, &blend, ULW_ALPHA);
    dialog->bar_drawn = bar_pixels(dialog, done);
}

static void answer(void) {
    MSG message;
    while (PeekMessageW(&message, NULL, 0, 0, PM_REMOVE)) {
        TranslateMessage(&message);
        DispatchMessageW(&message);
    }
}

UnpackDialog *unpack_dialog_open(const UnpackLook *look) {
    WNDCLASSW description = {0};
    UnpackDialog *dialog = calloc(1, sizeof *dialog);
    POINT pointer = {0, 0};
    HMONITOR monitor;
    MONITORINFO area = {0};
    UINT dpi_x = 96;
    UINT dpi_y = 96;
    BITMAPINFO format = {0};
    HDC screen;
    DPI_AWARENESS_CONTEXT previous;
    if (!dialog || prepare(look, &dialog->look) != 0) {
        free(dialog);
        return NULL;
    }
    dialog->title = _wcsdup(look->title ? look->title : L"");

    area.cbSize = sizeof area;
    format.bmiHeader.biSize = sizeof format.bmiHeader;
    /* We draw at the screen's own scale, and Windows does not stretch the
     * window, because only this window is DPI aware. */
    previous = SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    GetCursorPos(&pointer);
    monitor = MonitorFromPoint(pointer, MONITOR_DEFAULTTOPRIMARY);
    GetMonitorInfoW(monitor, &area);
    if (GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &dpi_x, &dpi_y) != S_OK)
        dpi_x = 96;
    dialog->scale = dpi_x / 96.0f;
    measure(dialog->scale, &dialog->width, &dialog->height);

    format.bmiHeader.biWidth = dialog->width;
    format.bmiHeader.biHeight = -dialog->height;
    format.bmiHeader.biPlanes = 1;
    format.bmiHeader.biBitCount = 32;
    format.bmiHeader.biCompression = BI_RGB;
    screen = GetDC(NULL);
    dialog->canvas_dc = CreateCompatibleDC(screen);
    ReleaseDC(NULL, screen);
    dialog->canvas_bitmap = CreateDIBSection(dialog->canvas_dc, &format, DIB_RGB_COLORS, &dialog->canvas_bits, NULL, 0);
    if (!dialog->canvas_bitmap) {
        SetThreadDpiAwarenessContext(previous);
        unpack_dialog_close(dialog);
        return NULL;
    }
    SelectObject(dialog->canvas_dc, dialog->canvas_bitmap);

    description.lpfnWndProc = DefWindowProcW;
    description.hInstance = GetModuleHandleW(NULL);
    description.lpszClassName = window_class;
    description.hCursor = LoadCursorW(NULL, (LPCWSTR)IDC_APPSTARTING);
    RegisterClassW(&description);
    dialog->window = CreateWindowExW(WS_EX_LAYERED | WS_EX_TOOLWINDOW, window_class, dialog->title, WS_POPUP,
                                     (area.rcWork.left + area.rcWork.right - dialog->width) / 2,
                                     (area.rcWork.top + area.rcWork.bottom - dialog->height) / 2, dialog->width,
                                     dialog->height, NULL, NULL, GetModuleHandleW(NULL), NULL);
    SetThreadDpiAwarenessContext(previous);
    if (!dialog->window) {
        unpack_dialog_close(dialog);
        return NULL;
    }
    present(dialog, 0);
    ShowWindow(dialog->window, SW_SHOWNOACTIVATE);
    answer();
    return dialog;
}

void unpack_dialog_progress(UnpackDialog *dialog, double done) {
    if (!dialog)
        return;
    if (bar_pixels(dialog, done) != dialog->bar_drawn)
        present(dialog, done);
    answer();
}

void unpack_dialog_close(UnpackDialog *dialog) {
    if (!dialog)
        return;
    if (dialog->window)
        DestroyWindow(dialog->window);
    if (dialog->canvas_bitmap)
        DeleteObject(dialog->canvas_bitmap);
    if (dialog->canvas_dc)
        DeleteDC(dialog->canvas_dc);
    forget(&dialog->look);
    free(dialog->title);
    free(dialog);
}

int unpack_dialog_picture(const UnpackLook *given, double done, unsigned dpi, const wchar_t *png) {
    /* image/png's encoder. */
    static const CLSID png_encoder = {0x557cf406, 0x1a04, 0x11d3, {0x9a, 0x73, 0x00, 0x00, 0xf8, 0x1e, 0xf3, 0x2e}};
    Look look;
    GpBitmap *canvas;
    GpGraphics *graphics;
    float scale = dpi / 96.0f;
    int width;
    int height;
    int written = -1;
    if (prepare(given, &look) != 0)
        return -1;
    measure(scale, &width, &height);
    if (GdipCreateBitmapFromScan0(width, height, 0, PixelFormat32bppPARGB, NULL, &canvas) == Ok) {
        if (GdipGetImageGraphicsContext((GpImage *)canvas, &graphics) == Ok) {
            draw(graphics, &look, given->title ? given->title : L"", done, scale);
            GdipDeleteGraphics(graphics);
            written = GdipSaveImageToFile((GpImage *)canvas, png, &png_encoder, NULL) == Ok ? 0 : -1;
        }
        GdipDisposeImage((GpImage *)canvas);
    }
    forget(&look);
    return written;
}
