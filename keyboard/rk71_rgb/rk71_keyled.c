// Copyright 2026 josemanuque
// SPDX-License-Identifier: GPL-2.0-or-later
//
// Brightness scaling between QMK's RGB matrix and the SN32 LED driver, for "dim on battery".
//
// The linker swaps every call to sn32f2xx_set_color(_all) for the __wrap_ versions below
// (-Wl,--wrap in rules.mk), which scale the colour and pass it on to the real driver. Scaling here,
// rather than lowering the RGB matrix brightness, works with every effect and indicator and never
// touches the brightness the user saved. (A custom RGB matrix driver is not an option: the key scan
// is only compiled in when the sn32f2xx driver is selected.)
#include "rk71_keyled.h"
#include "lib/lib8tion/lib8tion.h"

static uint8_t scale = 255; /* 255 = unchanged */

void rk71_keyled_scale(uint8_t new_scale) {
    scale = new_scale;
}

void __real_sn32f2xx_set_color(int index, uint8_t r, uint8_t g, uint8_t b);
void __real_sn32f2xx_set_color_all(uint8_t r, uint8_t g, uint8_t b);

void __wrap_sn32f2xx_set_color(int index, uint8_t r, uint8_t g, uint8_t b) {
    __real_sn32f2xx_set_color(index, scale8(r, scale), scale8(g, scale), scale8(b, scale));
}

void __wrap_sn32f2xx_set_color_all(uint8_t r, uint8_t g, uint8_t b) {
    __real_sn32f2xx_set_color_all(scale8(r, scale), scale8(g, scale), scale8(b, scale));
}
