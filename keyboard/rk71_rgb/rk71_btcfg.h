// Copyright 2026 josemanuque
// SPDX-License-Identifier: GPL-2.0-or-later
#pragma once

#include <stdbool.h>
#include <stdint.h>

/* Feed every lock-LED state the Bluetooth module forwards (`B1 xx`). */
void btcfg_leds(uint8_t leds, uint32_t now);

/* Called with each message that arrived intact: a VIA value write (channel, id, two value bytes). */
void rk71_btcfg_apply(uint8_t channel, uint8_t id, uint8_t v0, uint8_t v1);

/* Called after each applied message: send this consumer usage, then a release, to the host. */
void rk71_btcfg_ack(uint16_t usage);

/* Messages arrived within the last BTCFG_QUIET_MS: postpone flash writes, they would drop symbols. */
#define BTCFG_QUIET_MS 1500
bool btcfg_active(uint32_t now);

/* Diagnostics, read through VIA value 54. */
typedef struct {
    uint8_t  ok, bad;
    uint16_t symbols;
} btcfg_stats_t;
extern btcfg_stats_t btcfg_stats;
