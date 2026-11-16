// Copyright 2026 josemanuque
// SPDX-License-Identifier: GPL-2.0-or-later
#include "rk71_side.h"
#include <ch.h>
#include "gpio.h"
#include "timer.h"
#include "wait.h"

static const pin_t colour_pins[3]   = {D6, D4, D5};                /* red, green, blue */
static const pin_t position_pins[6] = {B9, B10, B11, B12, C0, C1}; /* far … near */

static uint8_t shown_positions = 0xFF; /* 0xFF: nothing shown yet, so the first side_show() applies */
static rgb_t   shown_colour;

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
 * Exact on-times: a 0.33 ms frame holds only ~20 system-timer ticks (5.3 us each) per colour,
 * and the timer cannot schedule less than 2 ticks. So each phase is timed in two parts: whole
 * ticks by the timer, and the fraction of a tick (or a whole phase shorter than 2 ticks) by a
 * counted busy-wait inside the interrupt (~0.17 us steps, calibrated against the system timer at
 * start-up). Every frame is identical: no dithering, so no slow pulsing at low brightness
 * (dithering showed as visible pulsing there). The busy-waits take at most ~30 us of each
 * 330 us frame (all three colours very dim), and only while the side lights are on.
 */
#define FRAME_TICKS TIME_US2I(330)
#define MIN_TICKS 2                               /* shortest delay the system timer allows */
#define MAX_TICKS ((FRAME_TICKS - MIN_TICKS) / 3) /* one colour at full balance and brightness */
#define FRACTION 256                              /* on-times are kept in 1/256 tick */

static uint16_t        target[3]; /* on-time per colour, in 1/256 tick */
static uint8_t         phase;     /* 0 red, 1 green, 2 blue, 3 off */
static uint8_t         frame_used;    /* timer ticks used by this frame's colour phases */
static uint32_t        tail_loops;    /* busy-wait to finish the current phase with */
static uint32_t        loops_per_tick = 1;
static virtual_timer_t frame_timer;
static uint8_t         balance[3] = {31, 31, 31}; /* 0..31 per colour */
static uint8_t         level      = 255;          /* brightness 0..255 */

/* Delay loop. Not inlined, so there is exactly one copy of it and calibrate_spin() measures the very
 * code every phase uses; the empty asm keeps the compiler from removing the loop. */
__attribute__((noinline)) static void spin(uint32_t loops) {
    while (loops--) __asm__ volatile("");
}

static uint32_t fraction_loops(uint32_t t) {
    return t * loops_per_tick / FRACTION;
}

/* How many delay-loop iterations make one system-timer tick (measured, so it is right whatever the
 * flash wait states or clock are). Interrupts are off so nothing stretches the measurement. */
static void calibrate_spin(void) {
    const uint32_t loops = 20000; /* ~1.7 ms, ~300 ticks */
    chSysLock();
    systime_t start = chVTGetSystemTimeX();
    spin(loops);
    sysinterval_t ticks = chTimeDiffX(start, chVTGetSystemTimeX());
    chSysUnlock();
    loops_per_tick = ticks ? loops / ticks : 1;
}

static void phase_tick(virtual_timer_t *vtp, void *p) {
    (void)p;
    spin(tail_loops); /* finish the phase that is ending: its fraction of a tick */
    tail_loops = 0;
    for (uint8_t i = 0; i < 3; i++) gpio_write_pin_low(colour_pins[i]);

    for (;;) {
        phase = (phase + 1) % 4;
        if (phase == 0) frame_used = 0;
        if (phase == 3) {
            chVTSetI(vtp, FRAME_TICKS - frame_used, phase_tick, NULL); /* >= MIN_TICKS, see MAX_TICKS */
            return;
        }
        uint16_t t = target[phase];
        if (!t) continue;
        uint8_t whole = t / FRACTION;
        gpio_write_pin_high(colour_pins[phase]);
        if (whole < MIN_TICKS) {
            spin(fraction_loops(t)); /* too short for the timer: the whole phase here */
            gpio_write_pin_low(colour_pins[phase]);
            continue;
        }
        tail_loops = fraction_loops(t % FRACTION);
        frame_used += whole;
        chVTSetI(vtp, whole, phase_tick, NULL);
        return;
    }
}

static void recompute(void) {
    const uint8_t want[3] = {shown_colour.r, shown_colour.g, shown_colour.b};
    uint16_t      t[3];
    for (uint8_t c = 0; c < 3; c++) {
        t[c] = shown_positions == 0xFF ? 0 : (uint32_t)MAX_TICKS * FRACTION * balance[c] * level / (31u * 255u) * want[c] / 255u;
    }
    chSysLock();
    for (uint8_t c = 0; c < 3; c++) target[c] = t[c];
    chSysUnlock();
}

/* The frame timer only runs while something is lit, so a dark keyboard has no timer wake-ups. */
static void frame_timer_run(bool run) {
    chSysLock();
    if (run && !chVTIsArmedI(&frame_timer)) {
        phase      = 3; /* "off" phase: the next tick starts a fresh frame with red */
        tail_loops = 0;
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
    calibrate_spin();
    side_show(0, SIDE_OFF);
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

static bool is_off(rgb_t c) {
    return !c.r && !c.g && !c.b;
}

void side_show(uint8_t positions, rgb_t colour) {
    if (!positions || is_off(colour)) {
        positions = 0;
        colour    = SIDE_OFF;
    }
    if (positions == shown_positions && colour.r == shown_colour.r && colour.g == shown_colour.g && colour.b == shown_colour.b) return;
    shown_positions = positions;
    shown_colour    = colour;
    recompute(); /* the colour lines are driven by phase_tick() */
    for (uint8_t i = 0; i < 6; i++) gpio_write_pin(position_pins[i], !((positions >> i) & 1));
    frame_timer_run(positions != 0);
}

void side_blank(void) {
    chSysLock();
    for (uint8_t i = 0; i < 3; i++) gpio_write_pin_low(colour_pins[i]); /* the next frame relights them */
    chSysUnlock();
}

bool side_is_dark(void) {
    return shown_positions == 0;
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
