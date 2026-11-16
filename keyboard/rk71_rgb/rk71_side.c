// Copyright 2026 josemanuque
// SPDX-License-Identifier: GPL-2.0-or-later
#include "rk71_side.h"
#include <ch.h>
#include "gpio.h"
#include "timer.h"
#include "wait.h"

static const pin_t colour_pins[3]   = {D6, D4, D5};                /* red, green, blue */
static const pin_t position_pins[6] = {B9, B10, B11, B12, C0, C1}; /* far … near */

static uint8_t shown_positions = 0xFF, shown_colour = 0xFF;

/*
 * Red, green and blue are lit one after another, never together: the LEDs share a supply per
 * position, and with red lit (lowest forward voltage) green and blue cannot light ("white" came
 * out red). Each frame is: red on, green on, blue on, then off for the rest. The on-times set
 * both the colour balance (the side LEDs sit behind a bluish diffuser) and the brightness.
 *
 * Frame rate: ~3 kHz (0.33 ms frames). Lit one colour at a time and fully on/off, the lights
 * must cycle fast or the separate colours show as a flicker or rainbow fringe when the eyes move
 * (seen at 0.83 kHz). IEEE 1789-2015 rates 100 % modulation as low-risk from 1.25 kHz and as
 * having no observable effect from 3 kHz.
 *
 * Dithering: a 0.33 ms frame holds only ~20 system-timer ticks (5.3 us each) per colour, too
 * coarse for the balance and brightness settings. So each colour's on-time is kept in 1/256 tick
 * and every frame rounds it up or down so that the average is exact (first-order sigma-delta).
 * Neighbouring frames differ by one tick, a ~5 % ripple at a few hundred Hz or more: far below
 * anything visible.
 */
#define FRAME_TICKS TIME_US2I(330)
#define MAX_TICKS ((FRAME_TICKS - MIN_TICKS) / 3) /* one colour at full balance and brightness */
#define MIN_TICKS 2                               /* shortest delay the system timer allows */
#define FRACTION 256                              /* on-times are kept in 1/256 tick */

static uint16_t        target[3];    /* wanted on-time per colour, in 1/256 tick */
static uint16_t        residue[3];   /* sigma-delta accumulators */
static uint8_t         frame_ticks[4]; /* this frame: red, green, blue, off */
static uint8_t         phase;
static virtual_timer_t frame_timer;
static uint8_t         balance[3] = {31, 31, 31}; /* 0..31 per colour */
static uint8_t         level      = 255;          /* brightness 0..255 */

/* Plan the next frame: whole ticks per colour, carrying the remainder into the next frame. */
static void plan_frame(void) {
    uint8_t used = 0;
    for (uint8_t c = 0; c < 3; c++) {
        residue[c] += target[c];
        uint8_t n = residue[c] / FRACTION;
        if (n < MIN_TICKS) n = 0;         /* too short for the timer: carry it over until it fits */
        if (n > MAX_TICKS) n = MAX_TICKS; /* after a jump from dim to bright the carry can overshoot */
        residue[c] -= n * FRACTION;
        frame_ticks[c] = n;
        used += n;
    }
    frame_ticks[3] = FRAME_TICKS - used; /* always >= MIN_TICKS, see MAX_TICKS */
}

static void phase_tick(virtual_timer_t *vtp, void *p) {
    (void)p;
    for (uint8_t i = 0; i < 3; i++) gpio_write_pin_low(colour_pins[i]);
    do {
        phase = (phase + 1) % 4;
        if (phase == 0) plan_frame();
    } while (frame_ticks[phase] == 0); /* ends at the latest on the "off" phase */
    if (phase < 3) gpio_write_pin_high(colour_pins[phase]);
    chVTSetI(vtp, frame_ticks[phase], phase_tick, NULL);
}

static void recompute(void) {
    uint16_t t[3];
    for (uint8_t c = 0; c < 3; c++) {
        t[c] = 0;
        if ((shown_colour >> c) & 1 && shown_colour != 0xFF) {
            t[c] = (uint32_t)MAX_TICKS * FRACTION * balance[c] * level / (31u * 255u);
        }
    }
    chSysLock();
    for (uint8_t c = 0; c < 3; c++) target[c] = t[c];
    chSysUnlock();
}

/* The frame timer only runs while something is lit, so a dark keyboard has no timer wake-ups. */
static void frame_timer_run(bool run) {
    chSysLock();
    if (run && !chVTIsArmedI(&frame_timer)) {
        phase          = 3; /* "off" phase: the next tick plans a fresh frame and starts with red */
        frame_ticks[3] = MIN_TICKS;
        chVTSetI(&frame_timer, MIN_TICKS, phase_tick, NULL);
    } else if (!run && chVTIsArmedI(&frame_timer)) {
        chVTResetI(&frame_timer);
        for (uint8_t i = 0; i < 3; i++) gpio_write_pin_low(colour_pins[i]);
    }
    chSysUnlock();
}

void side_init(void) {
    gpio_set_pin_output(B8); /* stock keeps B8 high; low makes Win/Alt glow faintly */
    gpio_write_pin_high(B8);
    for (uint8_t i = 0; i < 3; i++) {
        gpio_set_pin_output(colour_pins[i]);
        gpio_write_pin_low(colour_pins[i]);
    }
    for (uint8_t i = 0; i < 6; i++) gpio_set_pin_output(position_pins[i]);
    chVTObjectInit(&frame_timer);
    side_show(0, 0);
}

void side_set_tuning(uint8_t red, uint8_t green, uint8_t blue) {
    balance[0] = red & 31;
    balance[1] = green & 31;
    balance[2] = blue & 31;
    recompute();
}

void side_set_level(uint8_t new_level) {
    if (new_level == level) return;
    level = new_level;
    recompute();
}

void side_show(uint8_t positions, uint8_t colour) {
    if (!positions || !colour) positions = colour = 0;
    if (positions == shown_positions && colour == shown_colour) return;
    shown_positions = positions;
    shown_colour    = colour;
    recompute(); /* the colour lines are driven by phase_tick() */
    for (uint8_t i = 0; i < 6; i++) gpio_write_pin(position_pins[i], !((positions >> i) & 1));
    frame_timer_run(colour != 0);
}

bool side_is_dark(void) {
    return shown_colour == 0;
}

/* ---- cable detection --------------------------------------------------------------------------
 * C15 sees the cable's power through a resistor but has no pull-down (and the chip has none), so
 * once unplugged it only drifts low over seconds. Each sample therefore drains the pin for a few
 * microseconds, then reads it: with the cable in it is pulled straight back up. */

#define VBUS_PIN C15
#define SAMPLE_MS 20
#define STABLE_SAMPLES 3

static uint32_t last_sample;
static uint8_t  same_count;
static bool     raw_last, state, known;

void vbus_task(void) {
    if (timer_elapsed32(last_sample) < SAMPLE_MS) return;
    last_sample = timer_read32();

    gpio_set_pin_output(VBUS_PIN);
    gpio_write_pin_low(VBUS_PIN);
    wait_us(5);
    gpio_set_pin_input(VBUS_PIN);
    wait_us(30);
    bool raw = gpio_read_pin(VBUS_PIN);

    if (raw == raw_last) {
        if (same_count < STABLE_SAMPLES) same_count++;
    } else {
        same_count = 0;
        raw_last   = raw;
    }
    if (same_count >= STABLE_SAMPLES) {
        state = raw;
        known = true;
    }
}

bool vbus_present(void) {
    return state;
}

bool vbus_known(void) {
    return known;
}
