// Copyright 2026 josemanuque
// SPDX-License-Identifier: GPL-2.0-or-later
#pragma once

/*
 * Deep sleep on battery, done the way stock firmware V1.12 does it (@0x01F0): stop the LED/key
 * scan, put the matrix in a "any key pulls a column low" state, arm GPIO interrupts on the
 * columns, set PMU mode 2 (Deep-sleep) and WFI. The clocks stop, so the chip draws next to
 * nothing until a key is pressed, the USB cable is plugged in, or the Bluetooth module has
 * something to say (it raises A1 first).
 *
 * Call rk71_sleep_init() once at start-up. rk71_sleep() returns after waking up; the waking key is
 * still held, so the normal matrix scan picks it up and nothing is lost.
 * The caller must make sure nothing is lit and no key is held before calling it.
 */

void rk71_sleep_init(void);
void rk71_sleep(void);
