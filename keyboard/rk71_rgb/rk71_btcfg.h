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

/* Sends a usage to the host over Bluetooth; false if the module's queue is full. */
bool rk71_btcfg_usage(uint16_t usage);

/* Values reported to the app, as VIA reads them: key brightness, effect, speed, hue, saturation, the
 * Mac layout per device, and the Bluetooth device slot in use (0..4). */
#define BTCFG_SLOTS 7
void rk71_btcfg_read(uint8_t values[BTCFG_SLOTS]);

/* Call every 100 ms or so: reports one value that changed on the keyboard (Fn keys) while `connected`. */
void btcfg_task(uint32_t now, bool connected);

/* A message on this channel asks for every value to be reported. */
#define BTCFG_SYNC_CHANNEL 0xF0

/* Messages arrived within the last BTCFG_QUIET_MS: postpone flash writes, they would drop symbols. */
#define BTCFG_QUIET_MS 1500
bool btcfg_active(uint32_t now);

/* Diagnostics, read through VIA value 54. */
typedef struct {
    uint8_t  ok, bad;
    uint16_t symbols;
} btcfg_stats_t;
extern btcfg_stats_t btcfg_stats;
