#pragma once

/* The platform part of the menu GL probe: a GL context made current on a
 * window that is never shown. The checks are the same on every platform
 * (menu_gl_probe.cpp), and each platform has its own file for this part. */

struct ProbeContext;

/* Once, before any context. */
void probe_platform_start();

/* A core profile (3.2) or a legacy context, current on return, or null when
 * the platform has no way to make one. Once it returns, the GL entry points
 * of the menu renderer are usable. */
ProbeContext *probe_context_create(bool core);

void probe_context_release(ProbeContext *context);
