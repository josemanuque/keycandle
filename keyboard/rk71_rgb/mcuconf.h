// Copyright 2026 josemanuque
// SPDX-License-Identifier: GPL-2.0-or-later
#pragma once

#include_next <mcuconf.h>

/* The key LED driver (sn32f2xx.c) handles the CT16B1 timer interrupt itself: ChibiOS's generic PWM
 * handler checks all 24 channels and calls through a pointer before the row switch can start, which
 * is time the LEDs spend dark. ChibiOS still sets up and controls the timer (PWMD1). */
#define SN32_CT16B1_SUPPRESS_ISR
