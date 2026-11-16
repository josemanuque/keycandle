// Copyright 2026 josemanuque
// SPDX-License-Identifier: GPL-2.0-or-later
#pragma once

#include <stdbool.h>
#include <stdint.h>
#include "color.h"

/*
 * Side lights: 6 positions per side (1 = far end, 6 = near end), the same position lit on both
 * sides at once. Colour lines are shared, so all lit positions show the same colour.
 * Mapped with the diag keymap on a real board (2026-11-09):
 *   colour (high = on):    D6 red, D4 green, D5 blue
 *   position (low = on):   B9 B10 B11 B12 C0 C1  = positions 1..6
 * The stock firmware turns them off before sleeping (colours low, positions high, @0x0220).
 */

/* Any colour can be shown (each of red, green and blue 0..255); these are the named ones. */
#define SIDE_OFF ((rgb_t){0, 0, 0})
#define SIDE_RED ((rgb_t){255, 0, 0})
#define SIDE_GREEN ((rgb_t){0, 255, 0})
#define SIDE_YELLOW ((rgb_t){255, 255, 0})
#define SIDE_BLUE ((rgb_t){0, 0, 255})
#define SIDE_MAGENTA ((rgb_t){255, 0, 255})
#define SIDE_CYAN ((rgb_t){0, 255, 255})
#define SIDE_WHITE ((rgb_t){255, 255, 255})

#define SIDE_ALL 0x3F /* positions mask: bit 0 = position 1 (far) … bit 5 = position 6 (near) */

void side_init(void);
void side_show(uint8_t positions, rgb_t colour); /* positions 0 or SIDE_OFF = off */
/* Indicators on top (slot 0 or 1): these lights show their own colour instead of the main one.
 * positions 0 = no indicator. */
void side_indicator(uint8_t slot, uint8_t positions, rgb_t colour);
#define SIDE_NEAR 0x20 /* the light nearest you (position 6) */
#define SIDE_FAR 0x01  /* the farthest light (position 1) */
bool side_is_dark(void);                           /* nothing lit (and the frame timer stopped) */
void side_blank(void); /* colour lines off until the next frame (around a flash write, see rk71_leds_pause) */

/* Colour balance against the bluish diffuser: on-time of red, green, blue, 0..31 each. */
void side_set_tuning(uint8_t red, uint8_t green, uint8_t blue);
/* Overall brightness 0..255 (side-light brightness x keyboard brightness). */
void side_set_level(uint8_t level);
/* Experimental: light one position (left + right pair) at a time instead of all together. */
void side_set_scan(bool scan);

/* USB cable present (C15, high when the cable supplies power). Call vbus_task() from the main
 * loop; vbus_present() is debounced. */
void vbus_task(void);
bool vbus_present(void);
bool vbus_known(void);
