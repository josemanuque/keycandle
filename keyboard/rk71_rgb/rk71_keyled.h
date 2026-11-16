// Copyright 2026 josemanuque
// SPDX-License-Identifier: GPL-2.0-or-later
#pragma once

#include <stdint.h>

/* Scale all key LED colours (0..255, 255 = unchanged), e.g. to dim the lighting on battery. */
void rk71_keyled_scale(uint8_t scale);
