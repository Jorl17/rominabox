#pragma once

/* An OpenGL context for drawing the menu off screen, current on a window
 * that is never shown. We draw in it in the builder's preview renderer, and
 * check the renderer with it in the menu GL probe. Each platform has its own
 * file for it. */

struct OffscreenGl;

/* Once, before any context. */
void offscreen_gl_start();

/* A core profile (3.2) or a legacy context, current on return, or null when
 * the platform has no way to make one. Once it returns, the GL entry points
 * of the menu renderer are usable. */
OffscreenGl *offscreen_gl_create(bool core);

void offscreen_gl_release(OffscreenGl *context);
