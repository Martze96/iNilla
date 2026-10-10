/*
 * iNilla: a placeholder <os/activity.h> for compiling iOS objects on Linux. tracing-oslog's
 * `wrapper.h` includes it but its C code uses nothing from it (the activity API is bound from
 * Rust), so the declarations it needs live in the Rust bindings. See `log.h` beside this file.
 */
#ifndef INILLA_SHIM_OS_ACTIVITY_H
#define INILLA_SHIM_OS_ACTIVITY_H
#include <os/object.h>
#endif
