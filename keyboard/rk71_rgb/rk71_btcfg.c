// Copyright 2026 josemanuque
// SPDX-License-Identifier: GPL-2.0-or-later
/*
 * Settings over Bluetooth, both ways.
 *
 * Computer -> keyboard:
 *
 * Over Bluetooth the module gives the computer only keyboard and media interfaces; the one thing the
 * computer can send is the lock-LED state (Num, Caps, Scroll), which the module forwards as `B1 xx`.
 * Windows lets an ordinary app set a keyboard's indicator LEDs without touching the lock state
 * (IOCTL_KEYBOARD_SET_INDICATORS on the keyboard's own interface), so the Keycandle app sends
 * settings as a sequence of LED states:
 *
 *   symbol   one LED state; Scroll flips on every symbol (the clock), Num and Caps carry 2 bits
 *   message  4 start symbols (data 1, 2, 1, 2), then 5 bytes, 4 symbols each, high bits first:
 *            channel, value id, value byte 0, value byte 1, CRC-8 (poly 0x07) of the first four
 *
 * Symbols must follow each other within SYMBOL_GAP_MS; a pause, a malformed start or a bad CRC
 * drops the message. A message that arrives intact is applied and confirmed on the media-key
 * channel, which apps can read: usage 0x3F0 | (CRC & 0x0F), a reserved code hosts ignore, then a
 * release. The app resends a message until that confirmation comes (applying one twice is harmless).
 *
 * While messages are arriving the keyboard holds off its flash saves (btcfg_active): a flash write
 * stalls the chip long enough to lose an LED report from the module. People press lock keys far more slowly than that, so typing can't produce a
 * message, and the Caps key's light ignores LED changes shorter than 200 ms (rk71_rgb.c), so it
 * doesn't flicker while a message arrives.
 *
 * Keyboard -> computer: values changed here (Fn keys) go out on the same media-key channel, as two
 * reserved usages per value, each followed by a release:
 *
 *   0x300 | slot << 4 | high 4 bits,  then  0x380 + (slot << 4) | low 4 bits      (slot 0..6)
 *
 * A message on BTCFG_SYNC_CHANNEL asks for all of them (the app does when it finds the keyboard).
 * Nothing is reported while messages are arriving: sending could cost an LED report.
 */
#include "rk71_btcfg.h"
#include <string.h>
#include "timer.h"

#define SYMBOL_GAP_MS 250
#define LED_CLOCK 0x04 /* Scroll Lock */
#define LED_DATA 0x03  /* Num Lock (bit 0), Caps Lock (bit 1) */
#define START_SYMBOLS 4
#define MESSAGE_BYTES 5
#define SENT_KNOWN 0x80 /* in `requested`: `sent` holds real values (RAM is tight: no byte of its own) */

static const uint8_t start[START_SYMBOLS] = {1, 2, 1, 2};

static uint8_t  prev_leds = 0xFF;
static uint32_t prev_at; /* last LED report */
static uint8_t  symbols; /* of the current message */
static uint8_t  bytes[MESSAGE_BYTES];

static uint8_t sent[BTCFG_SLOTS]; /* what the computer was last told */
static uint8_t requested; /* slots to report even if unchanged, and SENT_KNOWN */

btcfg_stats_t btcfg_stats;

static uint8_t crc8(const uint8_t *data, uint8_t len) {
    uint8_t crc = 0;
    while (len--) {
        crc ^= *data++;
        for (uint8_t i = 0; i < 8; i++) crc = crc & 0x80 ? (uint8_t)(crc << 1) ^ 0x07 : (uint8_t)(crc << 1);
    }
    return crc;
}

static void restart(void) {
    symbols = 0;
}

void btcfg_leds(uint8_t leds, uint32_t now) {
    leds &= LED_CLOCK | LED_DATA;
    bool first  = prev_leds == 0xFF;
    bool clock  = !first && ((leds ^ prev_leds) & LED_CLOCK);
    bool paused = TIMER_DIFF_32(now, prev_at) > SYMBOL_GAP_MS;
    prev_leds   = leds;
    prev_at     = now;
    if (paused) restart();
    if (!clock) { /* a lock key, or the computer restoring its LEDs: not part of a message */
        restart();
        return;
    }
    btcfg_stats.symbols++;

    uint8_t data = leds & LED_DATA;
    if (symbols < START_SYMBOLS) {
        if (data == start[symbols]) {
            symbols++;
        } else {
            symbols = data == start[0]; /* maybe the start of a new message */
        }
        return;
    }
    uint8_t n    = symbols - START_SYMBOLS; /* data symbols so far */
    bytes[n / 4] = (uint8_t)(bytes[n / 4] << 2) | data;
    symbols++;
    if (n + 1 < MESSAGE_BYTES * 4) return;

    restart();
    if (crc8(bytes, MESSAGE_BYTES - 1) != bytes[MESSAGE_BYTES - 1]) {
        btcfg_stats.bad++;
        return;
    }
    btcfg_stats.ok++;
    if (bytes[0] == BTCFG_SYNC_CHANNEL) {
        requested |= (1 << BTCFG_SLOTS) - 1;
    } else {
        rk71_btcfg_apply(bytes[0], bytes[1], bytes[2], bytes[3]);
        rk71_btcfg_read(sent); /* the computer chose these: no need to tell it */
        requested |= SENT_KNOWN;
    }
    rk71_btcfg_ack(0x3F0 | (bytes[MESSAGE_BYTES - 1] & 0x0F));
}

bool btcfg_active(uint32_t now) {
    return btcfg_stats.symbols && TIMER_DIFF_32(now, prev_at) < BTCFG_QUIET_MS;
}

/* One value: both halves, each followed by a release. False if the module's queue filled up. */
static bool report(uint8_t slot, uint8_t value) {
    return rk71_btcfg_usage(0x300 | slot << 4 | value >> 4) && rk71_btcfg_usage(0) &&
           rk71_btcfg_usage((0x380 + (slot << 4)) | (value & 0x0F)) && rk71_btcfg_usage(0);
}

void btcfg_task(uint32_t now, bool connected) {
    uint8_t values[BTCFG_SLOTS];
    rk71_btcfg_read(values);
    if (!(requested & SENT_KNOWN)) { /* at start-up nothing has changed yet */
        memcpy(sent, values, BTCFG_SLOTS);
        requested |= SENT_KNOWN;
    }
    if (!connected || btcfg_active(now)) return;
    for (uint8_t slot = 0; slot < BTCFG_SLOTS; slot++) {
        if (values[slot] == sent[slot] && !(requested >> slot & 1)) continue;
        if (report(slot, values[slot])) {
            sent[slot] = values[slot];
            requested &= ~(1 << slot);
        }
        return; /* one per round */
    }
}
