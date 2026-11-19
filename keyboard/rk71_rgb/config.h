// Copyright 2026 josemanuque
// SPDX-License-Identifier: GPL-2.0-or-later
#pragma once

/* Firmware version, read by the configurator over VIA (id_firmware_version) to offer updates.
 * 0x00MMmmpp = major, minor, patch: 0x00000802 is v0.8.2. Bump it with every release tag. */
#define VIA_FIRMWARE_VERSION 0x00000900

/*
 * LED row pins, three per LED row in R, B, G order (the order the sn32f2xx driver expects).
 * Taken from stock firmware V1.12. Its LED step routine (0x5BFA) enables these pins together
 * with the red / blue / green brightness arrays at 0x20000962 / 0x20000B5A / 0x20000A5E.
 * This is the same pin set and order as royal_kludge/rk68_rgb (same HFD board family).
 *
 * Do NOT use C2 (P2.2): it is the BOOT pin. Driving it low as an LED output can force the
 * ROM bootloader on power-up (SN32F240B datasheet, section 1.4).
 */
#define SN32F2XX_RGB_MATRIX_ROW_PINS { C4, C5, C6, C7, C8, C9, C10, C11, C12, C13, C14, B13, D3, B15, B14 }

#define RGB_MATRIX_TYPING_HEATMAP_DECREASE_DELAY_MS 50

/*
 * Key LED refresh rate. The LED driver (sn32f2xx.c) lights one colour row at a time (15 rows: 5 key
 * rows x R/G/B), each for one PWM period of max_brightness (200) timer ticks plus the switch to the
 * next row (~21 us measured), and derives the tick rate from HUE_STEP * SAT_STEP * VAL_STEP *
 * SPD_STEP * LED_PROCESS_LIMIT (= 32768 * the limit), rounded by the timer prescaler (48 MHz / n).
 *
 * IEEE 1789 rates flicker at full modulation (the LEDs go fully off between rows) as low-risk from
 * 1250 Hz. 230 asks for 7.54 MHz, which the prescaler makes 48 MHz / 6 = 8 MHz: 25 us lit + ~21 us
 * switch (the side lights' timer may delay it) = ~46 us rows, ~1450 Hz per LED. (The default limit, 15, gave 165 Hz. Measure with
 * `keycandle --probe`. The limit itself only means the effects compute all 71 LEDs per pass
 * instead of a fifth at a time.)
 */
#define RGB_MATRIX_LED_PROCESS_LIMIT 230

/* Out-of-the-box settings, key lighting included: rk71_defaults.h. */
#include "rk71_defaults.h"

/*
 * Bluetooth module link (decoded from stock firmware V1.12): SPI0 slave on A2-A5, plus
 * A0 = keyboard -> module "packet ready" and A1 = module -> keyboard "module is sending".
 */
#define RK71_BT_REQ_PIN A0
#define RK71_BT_DIR_PIN A1

/* Saved keyboard settings (kb_data_t in rk71_rgb.c), with room to spare for new ones. */
#define EECONFIG_KB_DATA_SIZE 48

/* Layer numbers the keyboard code relies on (enum layers in the keymaps). */
#define RK71_WIN_LAYER 0  /* Windows base layer */
#define RK71_MAC_LAYER 1  /* Mac base layer */
#define RK71_FROW_LAYER 2 /* F-row lock */
#define RK71_FN_LAYER 3   /* Fn: while held, P and the device key show the connection */

/*
 * Battery saving (Bluetooth, no cable): the lighting turns off after a while without typing, and
 * once nothing is lit the chip deep-sleeps. Both times are set in VIA (Power); the defaults are 5
 * minutes and 10 seconds (rk71_rgb.c, lights_off_ms[] / sleep_ms[]).
 */
