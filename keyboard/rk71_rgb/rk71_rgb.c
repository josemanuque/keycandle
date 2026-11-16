// Copyright 2026 josemanuque
// SPDX-License-Identifier: GPL-2.0-or-later
//
// RK71 keyboard-level features. rk71_bt.c moves bytes to the Bluetooth module and rk71_side.c
// drives the side lights and reads the USB cable pin; this file adds:
//   - wired / Bluetooth mode: unplugged = Bluetooth; plugged = wired unless Fn+P chose Bluetooth
//   - the device slot (1-5) and the stock Fn+Q/W/E/R/T, Fn+P, Fn+Backspace controls (tap / 3 s hold)
//   - side lights: follow the keyboard colour, a fixed colour, or off (set in VIA), plus status:
//     pairing, connecting, mode changes, battery level (Fn+Space), low battery
//   - battery saving: lighting off when idle, then deep sleep (rk71_sleep.c)
//
// Commands (A6 xx yy) and events (B6 xx yy) were decoded from stock firmware V1.12. They match
// QMK's iton_bt.h: 51 81..85 select device, 51 89 pair, 51 70 forget all, 51 61 battery level,
// 58 01 wired mode; events 51 76/77/78/79 connected/pairing/disconnected/connecting,
// 5A 04/02/01 battery high/medium/low, 5A 06/07/0A battery low/shutdown/ok.
#include "rk71_rgb.h"
#include <string.h>
#include "rk71_bt.h"
#include "rk71_side.h"
#include "rk71_sleep.h"
#include "bluetooth.h"
#include "connection.h"
#ifdef VIA_ENABLE
#    include "via.h"
#endif

#define BT_CMD_BT 0x51
#define BT_CMD_USB 0x58
#define BT_SELECT_HOST(n) (0x80 + (n)) /* 51 81 ... 51 85 */
#define BT_PAIR 0x89
#define BT_FORGET_ALL 0x70
#define BT_QUERY_BATTERY 0x61
#define BT_USB_MODE 0x01

#define HOLD_MS 3000               /* hold time for pair / forget, as in the stock firmware */
#define BOOT_SETTLE_MS 1000        /* let the module boot before the first command */
#define BLINK_TIMEOUT_MS 60000     /* stop the pairing/connecting blink after this long */
#define LEDS_STABLE_MS 200         /* right after connecting, a wrong lock-LED value can arrive */
#define FLASH_MS 1500              /* mode-change flash */
#define BATTERY_SHOW_MS 3000       /* Fn+Space battery bar */
#define BATTERY_WAIT_MS 1500       /* no answer from the module after this long: show "unknown" */
#define FORGET_FLASH_MS 2000       /* "all pairings forgotten" flash */
#define LINK_DONE_MS 1000          /* connected: keep the solid blue this long */
#define SAVE_DELAY_MS 2000         /* VIA slider changes are saved once they settle */
#define WAKE_GRACE_MS 250          /* awake at least this long after waking: the key gets debounced
                                      and a plugged-in cable gets confirmed (4 x 20 ms samples) */

/* ---- saved settings ---------------------------------------------------------------------- */
/* Kept in the keyboard's EEPROM data block (EECONFIG_KB_DATA_SIZE in config.h). All-zero is the
 * default everywhere: wired, slot 1, side lights follow the keyboard, status shown. Settings whose
 * default is not 0 are stored XOR their default. */

enum { SIDE_MODE_FOLLOW, SIDE_MODE_FIXED, SIDE_MODE_OFF };

typedef union {
    uint32_t raw;
    struct {
        bool    bt_mode : 1;     /* when plugged in: 0 = wired, 1 = Bluetooth */
        uint8_t host : 3;        /* 0..4 -> device 1..5 */
        uint8_t side_mode : 2;   /* SIDE_MODE_FOLLOW / _FIXED / _OFF */
        uint8_t side_colour : 3; /* index into side_colours[] */
        bool    side_no_status : 1;
        /* Stored XOR the defaults below, so an all-zero config means "defaults". 0..31 each. */
        uint8_t side_red : 5;
        uint8_t side_green : 5;
        uint8_t side_blue : 5;
        uint8_t side_bright : 5;
    };
} kb_config_t;

static kb_config_t kb_config;

/* Settings added in v0.7b. */
#define SIDE_FLAG_OWN_BRIGHTNESS 0x01 /* side-light brightness does not follow the keyboard's */

typedef struct {
    uint8_t side_flags;    /* SIDE_FLAG_* */
    uint8_t status_bright; /* 0..31, XOR DEF_STATUS_BRIGHT */
} kb_extra_t;

static kb_extra_t kb_extra;

typedef struct {
    uint32_t   config; /* kb_config_t */
    kb_extra_t extra;
    uint8_t    reserved[EECONFIG_KB_DATA_SIZE - 4 - sizeof(kb_extra_t)];
} kb_data_t;
_Static_assert(sizeof(kb_data_t) == EECONFIG_KB_DATA_SIZE, "kb_data_t must fill EECONFIG_KB_DATA_SIZE");

/* Side-light defaults, tuned by eye on the real keyboard (2026-11-16): colour balance against the
 * diffuser, and a soft glow (the side lights are bright next to the keys at full power). */
#define DEF_RED 31
#define DEF_GREEN 15
#define DEF_BLUE 23
#define DEF_BRIGHT 2
#define SIDE_RED_V (kb_config.side_red ^ DEF_RED)
#define SIDE_GREEN_V (kb_config.side_green ^ DEF_GREEN)
#define SIDE_BLUE_V (kb_config.side_blue ^ DEF_BLUE)
#define SIDE_BRIGHT_V (kb_config.side_bright ^ DEF_BRIGHT)
/* Status (pairing, mode, battery) has its own brightness, so it stays readable when the side
 * lights are set to a faint glow or the keyboard lighting is dim. */
#define DEF_STATUS_BRIGHT 16
#define STATUS_BRIGHT_V (kb_extra.status_bright ^ DEF_STATUS_BRIGHT)

/* VIA dropdown order: White, Red, Yellow, Green, Cyan, Blue, Magenta */
static const rgb_t side_colours[] = {SIDE_WHITE, SIDE_RED, SIDE_YELLOW, SIDE_GREEN, SIDE_CYAN, SIDE_BLUE, SIDE_MAGENTA};
#define SIDE_COLOUR_COUNT (sizeof(side_colours) / sizeof(side_colours[0]))

static void load_config(void) {
    kb_data_t data;
    eeconfig_read_kb_datablock(&data, 0, sizeof(data));
    kb_config.raw = data.config;
    kb_extra      = data.extra;
}

static void save_config(void) {
    kb_data_t data = {.config = kb_config.raw, .extra = kb_extra};
    eeconfig_update_kb_datablock(&data, 0, sizeof(data));
}

void eeconfig_init_kb(void) {
    kb_config.raw = 0;
    kb_extra      = (kb_extra_t){0};
    save_config();
    eeconfig_init_user();
}

/* ---- link state ---------------------------------------------------------------------------- */

typedef enum { LINK_IDLE, LINK_CONNECTING, LINK_PAIRING, LINK_CONNECTED } link_state_t;

static link_state_t link_state;
static uint32_t     link_since;
static bool         link_connected;
static bool         battery_low;
static bool         battery_shutdown;
static uint8_t      battery_level; /* 0 unknown, 1 low, 2 medium, 3 high */
static uint8_t      host_leds;     /* lock LEDs (Num/Caps/Scroll) as the Bluetooth host reports them */
static uint8_t      pending_leds;  /* newest report, applied once stable for LEDS_STABLE_MS */
static uint32_t     pending_since;

static void set_link(link_state_t s) {
    link_state     = s;
    link_since     = timer_read32();
    link_connected = s == LINK_CONNECTED;
}

void rk71_bt_received(const uint8_t *p, uint8_t len) {
    if (p[0] == 0xB1) {
        pending_leds  = p[1] & 0x07;
        pending_since = timer_read32();
    } else if (p[0] == 0xB6 && p[1] == 0x51) {
        switch (p[2]) {
            case 0x76: set_link(LINK_CONNECTED); break;
            case 0x77: set_link(LINK_PAIRING); break;
            case 0x78: set_link(LINK_IDLE); break;
            case 0x79: set_link(LINK_CONNECTING); break;
        }
    } else if (p[0] == 0xB6 && p[1] == 0x5A) {
        switch (p[2]) {
            case 0x04: battery_level = 3; break;
            case 0x02: battery_level = 2; break;
            case 0x01: battery_level = 1; break;
            case 0x06: battery_low = true; break;
            case 0x07: battery_shutdown = true; break;
            case 0x0A: battery_low = battery_shutdown = false; break;
        }
    }
}

static void bt_command(uint8_t group, uint8_t param) {
    uint8_t pkt[3] = {0xA6, group, param};
    rk71_bt_send(pkt, sizeof(pkt));
}

/* ---- QMK Bluetooth driver hooks (BLUETOOTH_DRIVER = custom) -------------------------------- */

void bluetooth_init(void) {
    rk71_bt_init();
}

void bluetooth_task(void) {
    rk71_bt_task();
}

bool bluetooth_is_connected(void) {
    return link_connected;
}

uint8_t bluetooth_keyboard_leds(void) {
    return host_leds;
}

void bluetooth_send_keyboard(report_keyboard_t *report) {
    /* A1 + modifiers, reserved, 6 keys (boot keyboard report). */
    uint8_t pkt[9] = {0xA1, report->mods, 0};
    memcpy(&pkt[3], report->keys, 6);
    rk71_bt_send(pkt, sizeof(pkt));
}

void bluetooth_send_consumer(uint16_t usage) {
    uint8_t pkt[3] = {0xA3, usage >> 8, usage & 0xFF}; /* byte order as in QMK's iton_bt */
    rk71_bt_send(pkt, sizeof(pkt));
}

/* ---- mode handling -------------------------------------------------------------------------- */

/* A status display that runs for a fixed time after something happened. */
typedef struct {
    bool     active;
    uint32_t since;
} timed_t;

static void timed_start(timed_t *t) {
    t->active = true;
    t->since  = timer_read32();
}

static void timed_expire(timed_t *t, uint32_t now, uint32_t duration_ms) {
    if (t->active && TIMER_DIFF_32(now, t->since) > duration_ms) t->active = false;
}

enum { MODE_NONE = -1, MODE_WIRED, MODE_BLUETOOTH };

static uint32_t boot_time;
static int8_t   applied_mode = MODE_NONE; /* MODE_NONE until the first command reaches the module */
static timed_t  mode_flash;               /* wired/Bluetooth just changed */
static timed_t  forget_flash;             /* all pairings forgotten */
static timed_t  battery_bar;              /* Fn+Space: battery level requested */
static bool     show_link; /* connect/pair progress: only after a key press, not for automatic reconnects */

static bool on_bluetooth(void) {
    return applied_mode == MODE_BLUETOOTH;
}

static bool want_bt(void) {
    return !vbus_present() || kb_config.bt_mode; /* no cable: Bluetooth is the only option */
}

static void connect_host(void) {
    bt_command(BT_CMD_BT, BT_SELECT_HOST(kb_config.host + 1));
    set_link(LINK_CONNECTING);
}

static void apply_mode(void) {
    bool bt = want_bt();
    if (applied_mode == (bt ? MODE_BLUETOOTH : MODE_WIRED)) return;
    bool first   = applied_mode == MODE_NONE;
    applied_mode = bt ? MODE_BLUETOOTH : MODE_WIRED;
    if (bt) {
        host_leds = pending_leds = 0; /* no stale Caps Lock state from an earlier session */
        connection_set_host_noeeprom(CONNECTION_HOST_BLUETOOTH);
        connect_host();
    } else {
        connection_set_host_noeeprom(CONNECTION_HOST_USB);
        bt_command(BT_CMD_USB, BT_USB_MODE);
        set_link(LINK_IDLE);
    }
    if (!first) timed_start(&mode_flash);
}

static void select_host(uint8_t host, bool pair) {
    kb_config.host    = host;
    kb_config.bt_mode = true; /* choosing a device also means Bluetooth when plugged in */
    save_config();
    if (!on_bluetooth()) {
        apply_mode(); /* switches to Bluetooth and connects to the new slot */
    } else {
        connect_host();
    }
    if (pair) {
        bt_command(BT_CMD_BT, BT_PAIR);
        set_link(LINK_PAIRING);
    }
}

/* ---- keys: tap vs 3-second hold ------------------------------------------------------------- */

static uint16_t held_key;
static uint32_t held_since;
static bool     hold_done;

static void key_tap(uint16_t kc) {
    switch (kc) {
        case BT_HST1 ... BT_HST5:
            show_link = true;
            select_host(kc - BT_HST1, false);
            break;
        case BT_MODE:
            if (vbus_present()) { /* unplugged it is always Bluetooth */
                kb_config.bt_mode = !kb_config.bt_mode;
                save_config();
                apply_mode();
                show_link = on_bluetooth();
            }
            timed_start(&mode_flash);
            break;
    }
}

static void key_hold(uint16_t kc) {
    switch (kc) {
        case BT_HST1 ... BT_HST5:
            show_link = true;
            select_host(kc - BT_HST1, true);
            break;
        case BT_MODE:
            show_link = true;
            select_host(kb_config.host, true);
            break;
        case BT_CLR:
            bt_command(BT_CMD_BT, BT_FORGET_ALL);
            set_link(LINK_IDLE);
            timed_start(&forget_flash);
            break;
    }
}

bool process_record_kb(uint16_t keycode, keyrecord_t *record) {
    if (!process_record_user(keycode, record)) return false;
    if (keycode == BT_BATT) {
        if (record->event.pressed) {
            battery_level = 0;
            timed_start(&battery_bar);
            bt_command(BT_CMD_BT, BT_QUERY_BATTERY);
        }
        return false;
    }
    if (keycode >= BT_HST1 && keycode <= BT_CLR) {
        if (record->event.pressed) {
            held_key   = keycode;
            held_since = timer_read32();
            hold_done  = false;
        } else if (held_key == keycode) {
            if (!hold_done) key_tap(keycode);
            held_key = 0;
        }
        return false;
    }
    return true;
}

/* ---- side lights ---------------------------------------------------------------------------- */

static bool lighting_visible(void) {
    return rk71_rgb_lighting_on() && rgb_matrix_get_val() > 0;
}

/* The keyboard's colour (hue and saturation; brightness is applied separately). */
static rgb_t follow_colour(void) {
    hsv_t hsv = rgb_matrix_get_hsv();
    hsv.v     = 255;
    return hsv_to_rgb(hsv);
}

static bool blink(uint32_t now, uint16_t period) {
    return (now / period) % 2;
}

/* Low battery is shown only while typing, so it never keeps a nearly flat keyboard awake. */
static bool low_battery_warning(void) {
    return battery_low && last_input_activity_elapsed() < RK71_SLEEP_IDLE_MS;
}

/* Status takes over the side lights while something is happening. Returns false when idle. */
static bool side_status(uint32_t now, uint8_t *pos, rgb_t *col) {
    if (forget_flash.active) {
        *pos = SIDE_ALL;
        *col = blink(now, 150) ? SIDE_RED : SIDE_OFF;
        return true;
    }
    if (battery_bar.active) {
        if (battery_level) {
            static const uint8_t bars[] = {0, 0x20, 0x38, 0x3F}; /* 1 / 3 / 6 lights from the near end */
            static const rgb_t   cols[] = {SIDE_OFF, SIDE_RED, SIDE_YELLOW, SIDE_GREEN};
            *pos = bars[battery_level];
            *col = cols[battery_level];
        } else if (TIMER_DIFF_32(now, battery_bar.since) > BATTERY_WAIT_MS) {
            *pos = 0x20; /* no answer (slide switch off?): nearest light blinks red */
            *col = blink(now, 150) ? SIDE_RED : SIDE_OFF;
        } else {
            *pos = 0;
            *col = SIDE_OFF;
        }
        return true;
    }
    if (show_link && on_bluetooth()) {
        *pos = SIDE_ALL;
        switch (link_state) {
            case LINK_PAIRING: *col = blink(now, 200) ? SIDE_BLUE : SIDE_OFF; return true;
            case LINK_CONNECTED: *col = SIDE_BLUE; return true;
            default: *col = blink(now, 500) ? SIDE_BLUE : SIDE_OFF; return true; /* connecting */
        }
    }
    if (mode_flash.active) {
        *pos = SIDE_ALL;
        *col = on_bluetooth() ? SIDE_BLUE : SIDE_GREEN;
        return true;
    }
    if (low_battery_warning()) {
        *pos = 0x20; /* nearest light blinks red */
        *col = blink(now, 1000) ? SIDE_RED : SIDE_OFF;
        return true;
    }
    return false;
}

/* Side-light brightness 0..255: the setting, times the keyboard's brightness unless the side lights
 * have their own. */
static uint8_t side_level(void) {
    if (kb_extra.side_flags & SIDE_FLAG_OWN_BRIGHTNESS) return SIDE_BRIGHT_V * 255 / 31;
    uint32_t level = (uint32_t)SIDE_BRIGHT_V * rgb_matrix_get_val() * 255 / (31u * RGB_MATRIX_MAXIMUM_BRIGHTNESS);
    return level > 255 ? 255 : level;
}

static void side_update(uint32_t now) {
    uint8_t pos = 0;
    rgb_t   col = SIDE_OFF;
    if (!kb_config.side_no_status && side_status(now, &pos, &col)) {
        side_set_level(STATUS_BRIGHT_V * 255 / 31);
    } else {
        pos = 0;
        if (kb_config.side_mode != SIDE_MODE_OFF && lighting_visible()) {
            pos = SIDE_ALL;
            col = kb_config.side_mode == SIDE_MODE_FIXED ? side_colours[kb_config.side_colour % SIDE_COLOUR_COUNT] : follow_colour();
        }
        side_set_level(side_level());
    }
    side_show(pos, col);
}

/* ---- startup and background work ------------------------------------------------------------ */

void keyboard_pre_init_kb(void) {
    /* A6/A7 go to the Bluetooth module; the stock firmware keeps both as outputs, low. */
    gpio_set_pin_output(A6);
    gpio_write_pin_low(A6);
    gpio_set_pin_output(A7);
    gpio_write_pin_low(A7);
    side_init();
    keyboard_pre_init_user();
}

void keyboard_post_init_kb(void) {
    if (!eeconfig_is_kb_datablock_valid()) eeconfig_init_kb_datablock();
    load_config();
    if (kb_config.host > 4) kb_config.host = 0;
    if (kb_config.side_mode > SIDE_MODE_OFF) kb_config.side_mode = SIDE_MODE_FOLLOW;
    side_set_tuning(SIDE_RED_V, SIDE_GREEN_V, SIDE_BLUE_V);
    rk71_sleep_init();
    boot_time = timer_read32();
    keyboard_post_init_user();
}

static bool     rgb_off_for_idle;
static bool     save_pending; /* VIA settings changed, write to flash once they settle */
static uint32_t save_requested_at;
static uint32_t woke_at;

bool rk71_rgb_lighting_on(void) {
    return rgb_matrix_is_enabled() && !rgb_matrix_get_suspend_state();
}

static bool on_battery(void) {
    return on_bluetooth() && vbus_known() && !vbus_present();
}

/* A status display that should not be cut short by sleeping. */
static bool status_showing(void) {
    return forget_flash.active || battery_bar.active || mode_flash.active || (show_link && on_bluetooth());
}

static bool keys_held(void) {
    for (uint8_t row = 0; row < MATRIX_ROWS; row++) {
        if (matrix_get_row(row)) return true;
    }
    return held_key != 0;
}

/* Deep sleep: on battery, nothing lit, nothing pending, and no typing for a while. */
static bool may_sleep(uint32_t now) {
    return on_battery()                                          //
           && !lighting_visible() && side_is_dark()              // nothing lit
           && !status_showing()                                  //
           && !keys_held() && !save_pending && rk71_bt_idle()    // nothing pending
           && pending_leds == host_leds                          // Caps Lock update settled
           && last_input_activity_elapsed() >= RK71_SLEEP_IDLE_MS //
           && TIMER_DIFF_32(now, woke_at) >= WAKE_GRACE_MS;
}

void housekeeping_task_kb(void) {
    uint32_t now = timer_read32();

    vbus_task();

    if (pending_leds != host_leds && TIMER_DIFF_32(now, pending_since) >= LEDS_STABLE_MS) host_leds = pending_leds;

    if ((link_state == LINK_CONNECTING || link_state == LINK_PAIRING) && TIMER_DIFF_32(now, link_since) > BLINK_TIMEOUT_MS) {
        link_state = LINK_IDLE;
    }
    /* Progress display ends 1 s after connecting, or when the attempt times out. */
    bool link_done = link_state == LINK_CONNECTED && TIMER_DIFF_32(now, link_since) > LINK_DONE_MS;
    if (show_link && (link_done || TIMER_DIFF_32(now, link_since) > BLINK_TIMEOUT_MS || !on_bluetooth())) {
        show_link = false;
    }
    timed_expire(&mode_flash, now, FLASH_MS);
    timed_expire(&forget_flash, now, FORGET_FLASH_MS);
    timed_expire(&battery_bar, now, BATTERY_SHOW_MS);

    if (held_key && !hold_done && TIMER_DIFF_32(now, held_since) >= HOLD_MS) {
        hold_done = true;
        key_hold(held_key);
    }

    if (vbus_known() && TIMER_DIFF_32(now, boot_time) > BOOT_SETTLE_MS) apply_mode();

    /* Battery saving: on Bluetooth without a cable, lighting off after a while without typing,
     * and at once when the module reports a flat battery. */
    bool idle = on_battery() && (last_input_activity_elapsed() > RK71_LIGHTS_OFF_IDLE_MS || battery_shutdown);
    if (idle && !rgb_off_for_idle && rgb_matrix_is_enabled()) {
        rgb_matrix_disable_noeeprom();
        rgb_off_for_idle = true;
    } else if (!idle && rgb_off_for_idle) {
        rgb_matrix_enable_noeeprom();
        rgb_off_for_idle = false;
    }

    if (save_pending && TIMER_DIFF_32(now, save_requested_at) > SAVE_DELAY_MS) {
        save_pending = false;
        save_config();
    }

    side_update(now);

    if (may_sleep(now)) {
        rk71_sleep();
        woke_at = timer_read32();
    }

    housekeeping_task_user();
}

void suspend_power_down_kb(void) {
    side_show(0, SIDE_OFF); /* computer asleep: side lights off, as the stock firmware does */
    suspend_power_down_user();
}

/* ---- per-key RGB status (used when side-light status is turned off in VIA) ------------------- */

/* Matrix positions (row, column) of the keys used for status. */
#define KEY_BSPC 0, 13
#define KEY_P 1, 10
#define KEY_SPACE 4, 6
#define KEY_HOST(h) 1, (1 + (h)) /* Q W E R T */

static void light(uint8_t row, uint8_t col, uint8_t r, uint8_t g, uint8_t b, uint8_t led_min, uint8_t led_max) {
    uint8_t led = g_led_config.matrix_co[row][col];
    if (led != NO_LED && led >= led_min && led < led_max) rgb_matrix_set_color(led, r, g, b);
}

bool rgb_matrix_indicators_advanced_kb(uint8_t led_min, uint8_t led_max) {
    if (!rgb_matrix_indicators_advanced_user(led_min, led_max)) return false;

    uint32_t now = timer_read32();
    bool     bt  = on_bluetooth();

    if (kb_config.side_no_status) {
        if (bt && show_link) {
            switch (link_state) {
                case LINK_PAIRING:
                    if (blink(now, 200)) light(KEY_HOST(kb_config.host), RGB_BLUE, led_min, led_max);
                    break;
                case LINK_CONNECTED:
                    light(KEY_HOST(kb_config.host), RGB_BLUE, led_min, led_max);
                    break;
                default:
                    if (blink(now, 500)) light(KEY_HOST(kb_config.host), RGB_BLUE, led_min, led_max);
                    break;
            }
        }
        if (mode_flash.active && blink(now, 250)) {
            if (bt) {
                light(KEY_P, RGB_BLUE, led_min, led_max);
            } else {
                light(KEY_P, RGB_GREEN, led_min, led_max);
            }
        }
        if (forget_flash.active) light(KEY_BSPC, RGB_RED, led_min, led_max);
        if (low_battery_warning() && blink(now, 1000)) light(KEY_SPACE, RGB_RED, led_min, led_max);
    }

    /* While Fn is held: P shows the mode (green wired, blue Bluetooth), the slot key is blue. */
    if (IS_LAYER_ON(RK71_FN_LAYER)) {
        if (bt) {
            light(KEY_P, RGB_BLUE, led_min, led_max);
            light(KEY_HOST(kb_config.host), RGB_BLUE, led_min, led_max);
        } else {
            light(KEY_P, RGB_GREEN, led_min, led_max);
        }
    }
    return true;
}

/* ---- VIA: "Side lights" settings page --------------------------------------------------------- */
#ifdef VIA_ENABLE
enum {
    ID_SIDE_MODE = 1,
    ID_SIDE_COLOUR,
    ID_SIDE_STATUS,
    ID_SIDE_RED = 5,
    ID_SIDE_GREEN,
    ID_SIDE_BLUE,
    ID_SIDE_BRIGHT,
    ID_SIDE_FOLLOW_BRIGHTNESS, /* 1 = side brightness follows the keyboard's */
    ID_STATUS_BRIGHT,
};

void via_custom_value_command_kb(uint8_t *data, uint8_t length) {
    /* data = [ command_id, channel_id, value_id, value_data ] */
    uint8_t *command_id = &data[0];
    uint8_t  channel_id = data[1];
    uint8_t  value_id   = data[2];
    uint8_t *value      = &data[3];

    if (channel_id != id_custom_channel) {
        *command_id = id_unhandled;
        return;
    }
    switch (*command_id) {
        case id_custom_set_value:
            switch (value_id) {
                case ID_SIDE_MODE: kb_config.side_mode = value[0] > SIDE_MODE_OFF ? SIDE_MODE_FOLLOW : value[0]; break;
                case ID_SIDE_COLOUR: kb_config.side_colour = value[0] % SIDE_COLOUR_COUNT; break;
                case ID_SIDE_STATUS: kb_config.side_no_status = !value[0]; break;
                case ID_SIDE_RED: kb_config.side_red = (value[0] & 31) ^ DEF_RED; break;
                case ID_SIDE_GREEN: kb_config.side_green = (value[0] & 31) ^ DEF_GREEN; break;
                case ID_SIDE_BLUE: kb_config.side_blue = (value[0] & 31) ^ DEF_BLUE; break;
                case ID_SIDE_BRIGHT: kb_config.side_bright = (value[0] & 31) ^ DEF_BRIGHT; break;
                case ID_SIDE_FOLLOW_BRIGHTNESS:
                    if (value[0]) {
                        kb_extra.side_flags &= ~SIDE_FLAG_OWN_BRIGHTNESS;
                    } else {
                        kb_extra.side_flags |= SIDE_FLAG_OWN_BRIGHTNESS;
                    }
                    break;
                case ID_STATUS_BRIGHT: kb_extra.status_bright = (value[0] & 31) ^ DEF_STATUS_BRIGHT; break;
                default: *command_id = id_unhandled; break;
            }
            side_set_tuning(SIDE_RED_V, SIDE_GREEN_V, SIDE_BLUE_V);
            break;
        case id_custom_get_value:
            switch (value_id) {
                case ID_SIDE_MODE: value[0] = kb_config.side_mode; break;
                case ID_SIDE_COLOUR: value[0] = kb_config.side_colour; break;
                case ID_SIDE_STATUS: value[0] = !kb_config.side_no_status; break;
                case ID_SIDE_RED: value[0] = SIDE_RED_V; break;
                case ID_SIDE_GREEN: value[0] = SIDE_GREEN_V; break;
                case ID_SIDE_BLUE: value[0] = SIDE_BLUE_V; break;
                case ID_SIDE_BRIGHT: value[0] = SIDE_BRIGHT_V; break;
                case ID_SIDE_FOLLOW_BRIGHTNESS: value[0] = !(kb_extra.side_flags & SIDE_FLAG_OWN_BRIGHTNESS); break;
                case ID_STATUS_BRIGHT: value[0] = STATUS_BRIGHT_V; break;
                default: *command_id = id_unhandled; break;
            }
            break;
        case id_custom_save:
            /* VIA asks to save after every slider step. A flash write pauses the chip, which
             * makes the key lighting blink, so only write once the sliders have been still. */
            save_pending = true;
            save_requested_at = timer_read32();
            break;
        default:
            *command_id = id_unhandled;
            break;
    }
}
#endif
