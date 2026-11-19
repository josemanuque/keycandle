// Copyright 2026 josemanuque
// SPDX-License-Identifier: GPL-2.0-or-later
/*
 * Out-of-the-box settings: what the keyboard uses until they're changed (VIA, the Keycandle app or
 * the Fn keys), and again after a settings reset. Every default lives here, so changing one never
 * means touching the code that uses it.
 *
 * Settings are stored XOR their default (an all-zero settings block means "all defaults"), so a
 * changed default also shifts what earlier saved values mean. That's harmless in practice: the
 * settings block resets whenever new firmware is installed, and the app puts your values back.
 */
#pragma once

/* ---- Key lighting (QMK RGB matrix): solid white ---- */
#define RGB_MATRIX_DEFAULT_MODE RGB_MATRIX_SOLID_COLOR
#define RGB_MATRIX_DEFAULT_HUE 0
#define RGB_MATRIX_DEFAULT_SAT 0

/* ---- Side lights ---- */
/* Colour balance against the diffuser, 0-31 each, tuned by eye on the real keyboard (2026-11-16). */
#define DEF_RED 31
#define DEF_GREEN 15
#define DEF_BLUE 23
/* A soft glow: the side lights are bright next to the keys at full power. 0-31. */
#define DEF_BRIGHT 2
/* Short-pulse compensation (red, green, blue), so white stays white at low brightness. 0-31. */
#define DEF_PULSE_RED 0
#define DEF_PULSE_GREEN 8
#define DEF_PULSE_BLUE 5
/* Side-light effect speed, 0-31. */
#define DEF_FX_SPEED 16
/* Status (pairing, mode, battery) has its own brightness, so it stays readable when the side lights
 * are a faint glow or the key lighting is dim. 0-31. */
#define DEF_STATUS_BRIGHT 16

/* ---- Colours (hue, saturation) ---- */
#define DEF_SIDE_FIXED ((hs_t){0, 0})      /* side lights, fixed colour: white */
#define DEF_CAPS_SIDE ((hs_t){0, 255})     /* Caps Lock on the side light: red */
#define DEF_FROW_SIDE ((hs_t){43, 255})    /* F-row lock on the side light: yellow */
#define DEF_CAPS_KEY ((hs_t){0, 0})        /* Caps key, custom colour: white */
#define DEF_STATUS_WIRED ((hs_t){85, 255}) /* status, wired: green */
#define DEF_STATUS_BT ((hs_t){170, 255})   /* status, Bluetooth: blue */

/* ---- Power (on battery) ---- */
/* Indexes into the time choices in rk71_rgb.c: lighting off after 1/2/5/10/30 min or never, deep
 * sleep after 5/10/30 s, 1/5 min or never. */
#define DEF_LIGHTS_OFF 2  /* 5 minutes */
#define DEF_SLEEP_AFTER 1 /* 10 seconds */
/* Lighting level on battery when dimming is on, 0-31. */
#define DEF_DIM_LEVEL 12 /* ~40 % */
