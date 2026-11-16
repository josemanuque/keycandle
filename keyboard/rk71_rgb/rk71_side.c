// Copyright 2026 josemanuque
// SPDX-License-Identifier: GPL-2.0-or-later
#include "rk71_side.h"
#include <ch.h>
#include "gpio.h"
#include "timer.h"
#include "wait.h"

static const pin_t colour_pins[3]   = {D6, D4, D5};                /* red, green, blue */
static const pin_t position_pins[6] = {B9, B10, B11, B12, C0, C1}; /* far … near */


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

/*
 * Up to three groups can be lit at once, each in its own colour: the main lights and two
 * indicators (e.g. Caps Lock on the nearest light). Within a colour phase all groups start
 * together and each switches off when its own on-time is up, so every group gets its exact colour.
 */
typedef struct {
    uint16_t t;         /* on-time in 1/256 tick */
    uint8_t  colour;    /* 0 red, 1 green, 2 blue */
    uint8_t  positions; /* lights on during this step */
} step_t;

#define GROUPS 3                 /* main lights + 2 indicators */
#define MAX_STEPS (3 * 6) /* per colour: one step per group switching off, or per light when scanning */

static step_t          steps[MAX_STEPS];
static uint8_t         n_steps, step_idx;
static uint8_t         lit_colour = 3; /* colour line currently on, 3 = none */
static uint8_t         frame_used;     /* timer ticks used by this frame's steps */
static uint32_t        tail_loops;     /* busy-wait that finishes the current step */
static uint32_t        loops_per_tick = 1;
static virtual_timer_t frame_timer;
static uint8_t         balance[3] = {31, 31, 31}; /* 0..31 per colour */
static uint8_t         level      = 255;          /* brightness 0..255 */

/*
 * Experimental "one light at a time": the 12 LEDs of a colour share its supply, so an LED with a
 * slightly lower forward voltage draws more than its share and the lights look uneven. Lighting
 * one position at a time (left + right pair) removes that sharing. Each position then gets up to a
 * sixth of the colour time, so its on-time is multiplied by SCAN_GAIN to make up for it.
 */
#define SCAN_GAIN 6
static bool scan_positions;

static uint8_t group_positions[GROUPS];
static rgb_t   group_colour[GROUPS];

/* Delay loop. Not inlined, so there is exactly one copy of it and calibrate_spin() measures the very
 * code every step uses; the empty asm keeps the compiler from removing the loop. */
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

static void colour_off(void) {
    for (uint8_t i = 0; i < 3; i++) gpio_write_pin_low(colour_pins[i]);
    lit_colour = 3;
}

static void set_positions(uint8_t positions) {
    for (uint8_t i = 0; i < 6; i++) gpio_write_pin(position_pins[i], !((positions >> i) & 1));
}

static void step_tick(virtual_timer_t *vtp, void *p) {
    (void)p;
    spin(tail_loops); /* finish the step that is ending: its fraction of a tick */
    tail_loops = 0;
    if (n_steps == 0) { /* went dark: frame_timer_run(false) has switched everything off */
        colour_off();
        return;
    }
    for (;;) {
        if (step_idx >= n_steps) { /* all colours done: dark for the rest of the frame */
            colour_off();
            step_idx = 0;
            chVTSetI(vtp, FRAME_TICKS - frame_used, step_tick, NULL); /* >= MIN_TICKS, see MAX_TICKS */
            frame_used = 0;
            return;
        }
        const step_t *s = &steps[step_idx++];
        if (s->colour != lit_colour) {
            colour_off(); /* never show the new positions in the old colour */
            set_positions(s->positions);
            gpio_write_pin_high(colour_pins[s->colour]);
            lit_colour = s->colour;
        } else {
            set_positions(s->positions); /* same colour: one group switches off */
        }
        uint8_t whole = s->t / FRACTION;
        if (whole < MIN_TICKS) {
            spin(fraction_loops(s->t)); /* too short for the timer: the whole step here */
            continue;
        }
        tail_loops = fraction_loops(s->t % FRACTION);
        frame_used += whole;
        chVTSetI(vtp, whole, step_tick, NULL);
        return;
    }
}

/* On-time of one colour component (0..255) at the current balance and brightness, in 1/256 tick. */
static uint16_t on_time(uint8_t colour, uint8_t component) {
    return (uint32_t)MAX_TICKS * FRACTION * balance[colour] * level / (31u * 255u) * component / 255u;
}

static uint8_t component(rgb_t c, uint8_t colour) {
    return colour == 0 ? c.r : colour == 1 ? c.g : c.b;
}

static void frame_timer_run(bool run);

static void recompute(void) {
    step_t  s[MAX_STEPS];
    uint8_t n = 0;

    /* Indicators replace the main colour on their lights. */
    uint8_t pos[GROUPS];
    pos[0] = group_positions[0];
    for (uint8_t g = 1; g < GROUPS; g++) {
        pos[g] = group_positions[g];
        pos[0] &= ~pos[g];
    }

    for (uint8_t c = 0; scan_positions && c < 3; c++) {
        for (uint8_t p = 0; p < 6; p++) {
            for (uint8_t g = 0; g < GROUPS; g++) {
                if (!((pos[g] >> p) & 1)) continue;
                uint32_t t = (uint32_t)on_time(c, component(group_colour[g], c)) * SCAN_GAIN;
                if (t > MAX_TICKS * FRACTION / 6) t = MAX_TICKS * FRACTION / 6;
                if (t) s[n++] = (step_t){t, c, 1 << p};
            }
        }
    }

    for (uint8_t c = 0; !scan_positions && c < 3; c++) {
        uint16_t t[GROUPS];
        uint8_t  on = 0; /* groups still lit in this colour */
        for (uint8_t g = 0; g < GROUPS; g++) {
            t[g] = pos[g] ? on_time(c, component(group_colour[g], c)) : 0;
            if (t[g]) on |= 1 << g;
        }
        uint16_t elapsed = 0;
        while (on) {
            uint16_t next = UINT16_MAX;
            uint8_t  lit  = 0;
            for (uint8_t g = 0; g < GROUPS; g++) {
                if (!((on >> g) & 1)) continue;
                lit |= pos[g];
                if (t[g] < next) next = t[g];
            }
            s[n++] = (step_t){next - elapsed, c, lit}; /* everyone still on, until the next one ends */
            elapsed = next;
            for (uint8_t g = 0; g < GROUPS; g++) {
                if (t[g] <= elapsed) on &= ~(1 << g);
            }
        }
    }
    chSysLock();
    for (uint8_t i = 0; i < n; i++) steps[i] = s[i];
    n_steps    = n;
    step_idx   = 0;
    frame_used = 0;
    chSysUnlock();
    frame_timer_run(n != 0);
}

/* The frame timer only runs while something is lit, so a dark keyboard has no timer wake-ups. */
static void frame_timer_run(bool run) {
    chSysLock();
    if (run && !chVTIsArmedI(&frame_timer)) {
        tail_loops = 0;
        chVTSetI(&frame_timer, MIN_TICKS, step_tick, NULL);
    } else if (!run && chVTIsArmedI(&frame_timer)) {
        chVTResetI(&frame_timer);
    }
    if (!run) {
        colour_off();
        set_positions(0);
    }
    chSysUnlock();
}

void side_init(void) {
    gpio_set_pin_output(B8); /* stock keeps B8 high; low makes Win/Alt glow faintly */
    gpio_write_pin_high(B8);
    for (uint8_t i = 0; i < 3; i++) gpio_set_pin_output(colour_pins[i]);
    for (uint8_t i = 0; i < 6; i++) gpio_set_pin_output(position_pins[i]);
    chVTObjectInit(&frame_timer);
    calibrate_spin();
    frame_timer_run(false);
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

static bool same(rgb_t a, rgb_t b) {
    return a.r == b.r && a.g == b.g && a.b == b.b;
}

static void set_group(uint8_t g, uint8_t positions, rgb_t colour) {
    if (!positions || is_off(colour)) {
        positions = 0;
        colour    = SIDE_OFF;
    }
    if (positions == group_positions[g] && same(colour, group_colour[g])) return;
    group_positions[g] = positions;
    group_colour[g]    = colour;
    recompute();
}

void side_show(uint8_t positions, rgb_t colour) {
    set_group(0, positions, colour);
}

void side_indicator(uint8_t slot, uint8_t positions, rgb_t colour) {
    if (slot < GROUPS - 1) set_group(1 + slot, positions, colour);
}

void side_set_scan(bool scan) {
    if (scan == scan_positions) return;
    scan_positions = scan;
    recompute();
}

void side_blank(void) {
    chSysLock();
    colour_off(); /* the next step relights them */
    chSysUnlock();
}

bool side_is_dark(void) {
    return n_steps == 0;
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
