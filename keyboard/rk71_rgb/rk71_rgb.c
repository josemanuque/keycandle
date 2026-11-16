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
#include "rk71_keyled.h"
#include "lib/lib8tion/lib8tion.h"
#include "bluetooth.h"
#include "connection.h"
#ifdef VIA_ENABLE
#    include "via.h"
#    include "raw_hid.h"
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
#define LOW_BATTERY_BAR_EVERY_MS 600000 /* automatic battery bar on "battery low": at most every 10 min */
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
#define SIDE_FLAG_CAPS 0x02           /* Caps Lock shown on the nearest side light */
#define SIDE_FLAG_FROW 0x04           /* F-row lock shown on the farthest side light */
#define KEY_FLAG_FN_HIGHLIGHT 0x01    /* while Fn is held, only keys with an Fn function stay lit */

/* Caps Lock key: Auto (stock cool white, or dark on white-ish lighting), Off, or a custom colour. */
enum { CAPS_KEY_AUTO, CAPS_KEY_OFF, CAPS_KEY_CUSTOM };

/* Side-light effects (applied to the main colour; status and indicators are not animated). */
enum { SIDE_FX_STATIC, SIDE_FX_BREATHING, SIDE_FX_CYCLE, SIDE_FX_PULSE, SIDE_FX_WAVE, SIDE_FX_COUNT };

/* A colour picked in VIA: hue and saturation (brightness has its own setting). Stored XOR its
 * default, so all-zero settings mean the defaults. */
typedef struct {
    uint8_t h, s;
} hs_t;

typedef struct {
    uint8_t side_flags;    /* SIDE_FLAG_* */
    uint8_t status_bright; /* 0..31, XOR DEF_STATUS_BRIGHT */
    uint8_t key_flags;     /* KEY_FLAG_* */
    uint8_t caps_key_mode; /* CAPS_KEY_* */
    uint8_t mac_devices;   /* bit n: device n is in Mac mode (0..4 Bluetooth slots, 5 wired) */
    uint8_t side_effect;   /* SIDE_FX_* */
    uint8_t side_fx_speed; /* 0..31, XOR DEF_FX_SPEED */
    hs_t    side_fixed;    /* "Fixed colour" of the side lights */
    hs_t    caps_side;     /* Caps Lock on the side lights */
    hs_t    frow_side;     /* F-row lock on the side lights */
    hs_t    caps_key;      /* Caps Lock key, CAPS_KEY_CUSTOM */
    hs_t    status_wired;  /* status colour for wired */
    hs_t    status_bt;     /* status colour for Bluetooth (connecting, pairing, mode) */
    uint8_t lights_off;    /* index into lights_off_ms[], XOR DEF_LIGHTS_OFF */
    uint8_t sleep_after;   /* index into sleep_ms[], XOR DEF_SLEEP_AFTER */
    uint8_t power_flags;   /* POWER_FLAG_* */
    uint8_t dim_level;     /* 0..31 brightness on battery, XOR DEF_DIM_LEVEL */
    uint8_t side_trim[6][3]; /* per-position red/green/blue strength 0..31, XOR 31 (0 = unchanged) */
    uint8_t pulse_extra[3];  /* side-light short-pulse compensation per colour, 0..63, XOR default */
} kb_extra_t;

#define POWER_FLAG_NO_AUTO_BATTERY 0x01 /* no battery bar on unplugging / low battery */
#define POWER_FLAG_DIM_ON_BATTERY 0x02  /* lighting dimmed to dim_level while unplugged */

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
#define DEF_FX_SPEED 16

/* Side-light short-pulse compensation (red, green, blue), tuned by eye on the real board so white
 * stays white at low brightness (2026-11-16). */
static const uint8_t def_pulse_extra[3] = {0, 8, 5};
#define PULSE_EXTRA_V(c) (kb_extra.pulse_extra[c] ^ def_pulse_extra[c])

/* Battery saving times (VIA "Power"). 0 = never. */
static const uint32_t lights_off_ms[] = {60000, 120000, 300000, 600000, 1800000, 0}; /* 1/2/5/10/30 min, never */
static const uint32_t sleep_ms[]      = {5000, 10000, 30000, 60000, 300000, 0};      /* 5/10/30 s, 1/5 min, never */
#define TIME_CHOICES 6
#define DEF_LIGHTS_OFF 2  /* 5 minutes */
#define DEF_SLEEP_AFTER 1 /* 10 seconds */
#define DEF_DIM_LEVEL 12  /* ~40 % */
#define LIGHTS_OFF_V ((kb_extra.lights_off ^ DEF_LIGHTS_OFF) % TIME_CHOICES)
#define SLEEP_AFTER_V ((kb_extra.sleep_after ^ DEF_SLEEP_AFTER) % TIME_CHOICES)
#define DIM_LEVEL_V (kb_extra.dim_level ^ DEF_DIM_LEVEL)
#define ACTIVE_WINDOW_MS 10000 /* "typing recently", when sleep is set to never */

static uint32_t sleep_after_ms(void) {
    return sleep_ms[SLEEP_AFTER_V];
}

/* Recently typed: indicators and the low-battery blink show only then (or with lighting on). */
static bool typing_recently(void) {
    uint32_t window = sleep_after_ms() ? sleep_after_ms() : ACTIVE_WINDOW_MS;
    return last_input_activity_elapsed() < window;
}
#define FX_SPEED_V (kb_extra.side_fx_speed ^ DEF_FX_SPEED)

/* Colour defaults (hue, saturation). */
#define DEF_SIDE_FIXED ((hs_t){0, 0})      /* white */
#define DEF_CAPS_SIDE ((hs_t){0, 255})     /* red */
#define DEF_FROW_SIDE ((hs_t){43, 255})    /* yellow */
#define DEF_CAPS_KEY ((hs_t){0, 255})      /* red */
#define DEF_STATUS_WIRED ((hs_t){85, 255}) /* green */
#define DEF_STATUS_BT ((hs_t){170, 255})   /* blue */

static hs_t hs_value(hs_t stored, hs_t def) {
    return (hs_t){stored.h ^ def.h, stored.s ^ def.s};
}

/* A stored colour at the given brightness (side lights: 255, their level is applied separately). */
static rgb_t hs_rgb(hs_t stored, hs_t def, uint8_t v) {
    hs_t c = hs_value(stored, def);
    return hsv_to_rgb((hsv_t){c.h, c.s, v});
}

#define STATUS_WIRED_RGB hs_rgb(kb_extra.status_wired, DEF_STATUS_WIRED, 255)
#define STATUS_BT_RGB hs_rgb(kb_extra.status_bt, DEF_STATUS_BT, 255)

static void load_config(void) {
    kb_data_t data;
    eeconfig_read_kb_datablock(&data, 0, sizeof(data));
    kb_config.raw = data.config;
    kb_extra      = data.extra;
}

/* Flash writes freeze the chip (it runs from flash), and frozen LEDs show a solid red, green or blue
 * flash. So the LEDs are switched off around every write: at most a very short dark blink. */
/* Set at the end of keyboard_post_init_kb(): settings loaded and the LED driver running. Before
 * that, eeconfig_init_kb() may already save, and QMK already sets the default layer. */
static bool kb_ready;

static void flash_write_begin(void) {
    if (!kb_ready) return;
    rk71_leds_pause();
    side_blank();
}

static void flash_write_end(void) {
    if (kb_ready) rk71_leds_resume();
}

static void save_config(void) {
    kb_data_t data = {.config = kb_config.raw, .extra = kb_extra};
    flash_write_begin();
    eeconfig_update_kb_datablock(&data, 0, sizeof(data));
    flash_write_end();
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
static bool         low_battery_event; /* "battery low" just arrived: maybe show the bar */
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
            case 0x06:
                battery_low       = true;
                low_battery_event = true;
                break;
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

/* ---- Windows / Mac per device ------------------------------------------------------------------
 * Fn+A / Fn+S pick the layout for the device in use; each Bluetooth slot and wired remember their
 * own, so switching from a PC to a Mac flips the modifiers by itself. */

#define WIRED_DEVICE 5


static uint8_t current_device(void) {
    return on_bluetooth() ? kb_config.host : WIRED_DEVICE;
}

static void apply_device_layout(void) {
    bool          mac  = (kb_extra.mac_devices >> current_device()) & 1;
    layer_state_t want = (layer_state_t)1 << (mac ? RK71_MAC_LAYER : RK71_WIN_LAYER);
    if (default_layer_state != want) default_layer_set(want);
}

layer_state_t default_layer_state_set_kb(layer_state_t state) {
    if (kb_ready) {
        uint8_t bit  = 1 << current_device();
        uint8_t devs = (state >> RK71_MAC_LAYER) & 1 ? kb_extra.mac_devices | bit : kb_extra.mac_devices & ~bit;
        if (devs != kb_extra.mac_devices) {
            kb_extra.mac_devices = devs;
            save_config();
        }
    }
    return default_layer_state_set_user(state);
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
    apply_device_layout();
}

static void select_host(uint8_t host, bool pair) {
    kb_config.host    = host;
    kb_config.bt_mode = true; /* choosing a device also means Bluetooth when plugged in */
    save_config();
    if (!on_bluetooth()) {
        apply_mode(); /* switches to Bluetooth and connects to the new slot */
    } else {
        connect_host();
        apply_device_layout();
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

/* Ask the module for the battery level and show it as a bar on the side lights (Fn+Space). */
static void show_battery_bar(void) {
    battery_level = 0;
    timed_start(&battery_bar);
    bt_command(BT_CMD_BT, BT_QUERY_BATTERY);
}

bool process_record_kb(uint16_t keycode, keyrecord_t *record) {
    if (!process_record_user(keycode, record)) return false;
    if (keycode == BT_BATT) {
        if (record->event.pressed) show_battery_bar();
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
    return battery_low && typing_recently();
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
            case LINK_PAIRING: *col = blink(now, 200) ? STATUS_BT_RGB : SIDE_OFF; return true;
            case LINK_CONNECTED: *col = STATUS_BT_RGB; return true;
            default: *col = blink(now, 500) ? STATUS_BT_RGB : SIDE_OFF; return true; /* connecting */
        }
    }
    if (mode_flash.active) {
        *pos = SIDE_ALL;
        *col = on_bluetooth() ? STATUS_BT_RGB : STATUS_WIRED_RGB;
        return true;
    }
    if (low_battery_warning()) {
        *pos = 0x20; /* nearest light blinks red */
        *col = blink(now, 1000) ? SIDE_RED : SIDE_OFF;
        return true;
    }
    return false;
}

static bool on_battery(void);

/* Side-light brightness 0..255: the setting, times the keyboard's brightness unless the side lights
 * have their own. */
static uint8_t side_level(void) {
    uint32_t level = SIDE_BRIGHT_V * 255 / 31;
    if (!(kb_extra.side_flags & SIDE_FLAG_OWN_BRIGHTNESS)) level = level * rgb_matrix_get_val() / RGB_MATRIX_MAXIMUM_BRIGHTNESS;
    if (on_battery() && (kb_extra.power_flags & POWER_FLAG_DIM_ON_BATTERY)) level = level * DIM_LEVEL_V / 31;
    return level > 255 ? 255 : level;
}

/* Caps Lock and F-row lock on the side lights. Shown while the lighting is on or while typing, so
 * a lock left on never keeps the keyboard from sleeping. */
static void side_indicators(bool show) {
    bool active = show && (lighting_visible() || typing_recently());
    bool caps   = active && (kb_extra.side_flags & SIDE_FLAG_CAPS) && host_keyboard_led_state().caps_lock;
    bool frow   = active && (kb_extra.side_flags & SIDE_FLAG_FROW) && layer_state_is(RK71_FROW_LAYER);
    side_indicator(0, caps ? SIDE_NEAR : 0, hs_rgb(kb_extra.caps_side, DEF_CAPS_SIDE, 255));
    side_indicator(1, frow ? SIDE_FAR : 0, hs_rgb(kb_extra.frow_side, DEF_FROW_SIDE, 255));
}

/* Effect period: speed 0..31 -> 4.0 s .. 0.9 s. */
static uint16_t fx_period(void) {
    return 4000 - FX_SPEED_V * 100;
}

/* Apply the side-light effect to the main lights: positions, colour and brightness. */
static void side_effect(uint32_t now, uint8_t *pos, rgb_t *col, uint8_t *level) {
    uint16_t period = fx_period();
    uint8_t  angle  = (now % period) * 256 / period; /* 0..255 through one period */
    switch (kb_extra.side_effect) {
        case SIDE_FX_BREATHING: /* 12 % .. 100 % */
            *level = scale8(*level, 32 + scale8(sin8(angle), 223));
            break;
        case SIDE_FX_CYCLE: /* drift through all hues */
            *col = hsv_to_rgb((hsv_t){angle, 255, 255});
            break;
        case SIDE_FX_PULSE: { /* faint, flaring up with each key press */
            uint32_t since = last_input_activity_elapsed();
            uint8_t  flare = since < period / 4 ? 255 - since * 255 / (period / 4) : 0;
            *level         = scale8(*level, 48 + scale8(flare, 207));
            break;
        }
        case SIDE_FX_WAVE: { /* two lights travelling far -> near -> far */
            uint8_t step = (now % period) * 8 / period; /* 0..7 */
            uint8_t at   = step < 5 ? step : 8 - step;  /* 0 1 2 3 4 3 2 1 */
            *pos         = 0x03 << at;
            break;
        }
        default:
            break;
    }
}

static bool side_calibrating; /* VIA "Calibrate": all side lights plain white (not saved) */

static void side_update(uint32_t now) {
    if (side_calibrating) {
        side_set_level(SIDE_BRIGHT_V * 255 / 31);
        side_indicators(false);
        side_show(SIDE_ALL, SIDE_WHITE);
        return;
    }
    uint8_t pos = 0;
    rgb_t   col = SIDE_OFF;
    if (!kb_config.side_no_status && side_status(now, &pos, &col)) {
        side_set_level(STATUS_BRIGHT_V * 255 / 31);
        side_indicators(false); /* status takes over all six lights */
    } else {
        pos = 0;
        if (kb_config.side_mode != SIDE_MODE_OFF && lighting_visible()) {
            pos = SIDE_ALL;
            col = kb_config.side_mode == SIDE_MODE_FIXED ? hs_rgb(kb_extra.side_fixed, DEF_SIDE_FIXED, 255) : follow_colour();
        }
        uint8_t level = side_level();
        if (pos) side_effect(now, &pos, &col, &level);
        side_set_level(level);
        side_indicators(true);
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
    for (uint8_t p = 0; p < 6; p++) {
        for (uint8_t c = 0; c < 3; c++) side_set_trim(p, c, kb_extra.side_trim[p][c] ^ 31);
    }
    for (uint8_t c = 0; c < 3; c++) side_set_pulse_extra(c, PULSE_EXTRA_V(c));
    rk71_sleep_init();
    kb_ready  = true;
    boot_time = timer_read32();
    keyboard_post_init_user();
}

static bool     rgb_off_for_idle;
static bool     save_pending;     /* VIA side-light settings changed: write once they settle */
static bool     rgb_save_pending; /* VIA key-lighting settings changed: same */
static uint32_t save_requested_at;
static uint32_t woke_at;

/* Write the settings VIA changed and that are still waiting to settle. */
static void flush_pending_saves(void) {
    if (save_pending) save_config();
    if (rgb_save_pending) {
        flash_write_begin();
        eeconfig_force_flush_rgb_matrix();
        flash_write_end();
    }
    save_pending = rgb_save_pending = false;
}

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
           && !keys_held() && !save_pending && !rgb_save_pending // nothing pending
           && rk71_bt_idle()                                     //
           && pending_leds == host_leds                          // Caps Lock update settled
           && sleep_after_ms() && last_input_activity_elapsed() >= sleep_after_ms() //
           && TIMER_DIFF_32(now, woke_at) >= WAKE_GRACE_MS;
}

/* Dim on battery, and the automatic battery bar (after unplugging, and on "battery low"). */
static void power_task(uint32_t now) {
    static bool     was_plugged;
    static uint32_t bar_due, last_low_bar;
    static bool     bar_pending;

    bool dim = on_battery() && (kb_extra.power_flags & POWER_FLAG_DIM_ON_BATTERY);
    rk71_keyled_scale(dim ? DIM_LEVEL_V * 255 / 31 : 255);

    bool plugged = vbus_known() && vbus_present();
    bool autobar = !(kb_extra.power_flags & POWER_FLAG_NO_AUTO_BATTERY);
    if (was_plugged && !plugged && autobar) {
        bar_pending = true; /* after the "switched to Bluetooth" flash */
        bar_due     = now + FLASH_MS + 200;
    }
    was_plugged = plugged;
    if (low_battery_event) {
        low_battery_event = false;
        if (autobar && (!last_low_bar || TIMER_DIFF_32(now, last_low_bar) > LOW_BATTERY_BAR_EVERY_MS)) {
            last_low_bar = now;
            bar_pending  = true;
            bar_due      = now;
        }
    }
    if (bar_pending && TIMER_DIFF_32(now, bar_due) < 0x80000000u) {
        bar_pending = false;
        if (on_bluetooth()) show_battery_bar();
    }
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
    uint32_t off_ms = lights_off_ms[LIGHTS_OFF_V];
    bool     idle   = on_battery() && ((off_ms && last_input_activity_elapsed() > off_ms) || battery_shutdown);
    if (idle && !rgb_off_for_idle && rgb_matrix_is_enabled()) {
        rgb_matrix_disable_noeeprom();
        rgb_off_for_idle = true;
    } else if (!idle && rgb_off_for_idle) {
        rgb_matrix_enable_noeeprom();
        rgb_off_for_idle = false;
    }

    if ((save_pending || rgb_save_pending) && TIMER_DIFF_32(now, save_requested_at) > SAVE_DELAY_MS) {
        flush_pending_saves();
    }

    power_task(now);
    side_update(now);

    if (may_sleep(now)) {
        rk71_sleep();
        woke_at = timer_read32();
    }

    housekeeping_task_user();
}

void suspend_power_down_kb(void) {
    side_show(0, SIDE_OFF); /* computer asleep: side lights off, as the stock firmware does */
    side_indicator(0, 0, SIDE_OFF);
    side_indicator(1, 0, SIDE_OFF);
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

#define KEY_CAPS 2, 0

/* Fn-layer highlight: while Fn is held, keys without an Fn function go dark (the Fn key stays). */
static void fn_highlight(uint8_t led_min, uint8_t led_max) {
    uint8_t base = get_highest_layer(default_layer_state);
    for (uint8_t row = 0; row < MATRIX_ROWS; row++) {
        for (uint8_t col = 0; col < MATRIX_COLS; col++) {
            keypos_t pos = {.row = row, .col = col};
            uint16_t fn  = keymap_key_to_keycode(RK71_FN_LAYER, pos);
            if (fn != KC_TRNS && fn != KC_NO) continue;
            if (keymap_key_to_keycode(base, pos) == MO(RK71_FN_LAYER)) continue;
            light(row, col, 0, 0, 0, led_min, led_max);
        }
    }
}

/* Win-key lock (Fn+Win): the locked GUI keys go dark. */
static void win_lock_dark(uint8_t led_min, uint8_t led_max) {
    uint8_t base = get_highest_layer(default_layer_state);
    for (uint8_t row = 0; row < MATRIX_ROWS; row++) {
        for (uint8_t col = 0; col < MATRIX_COLS; col++) {
            uint16_t kc = keymap_key_to_keycode(base, (keypos_t){.row = row, .col = col});
            if (kc == KC_LGUI || kc == KC_RGUI) light(row, col, 0, 0, 0, led_min, led_max);
        }
    }
}

static void caps_key(uint8_t led_min, uint8_t led_max) {
    if (!host_keyboard_led_state().caps_lock) return;
    uint8_t mode = kb_extra.caps_key_mode;
    if (mode == CAPS_KEY_OFF) return;
    if (mode == CAPS_KEY_CUSTOM) {
        rgb_t c = hs_rgb(kb_extra.caps_key, DEF_CAPS_KEY, RGB_MATRIX_MAXIMUM_BRIGHTNESS);
        light(KEY_CAPS, c.r, c.g, c.b, led_min, led_max);
    } else if (rgb_matrix_get_sat() < 64) {
        light(KEY_CAPS, 0, 0, 0, led_min, led_max); /* white-ish lighting: a dark key stands out */
    } else {
        light(KEY_CAPS, 170, 255, 255, led_min, led_max); /* stock's cool white (V1.12 @0xA914) */
    }
}

bool rgb_matrix_indicators_advanced_kb(uint8_t led_min, uint8_t led_max) {
    if (!rgb_matrix_indicators_advanced_user(led_min, led_max)) return false;

    uint32_t now = timer_read32();
    bool     bt  = on_bluetooth();
    rgb_t    sb  = hs_rgb(kb_extra.status_bt, DEF_STATUS_BT, RGB_MATRIX_MAXIMUM_BRIGHTNESS);
    rgb_t    sw  = hs_rgb(kb_extra.status_wired, DEF_STATUS_WIRED, RGB_MATRIX_MAXIMUM_BRIGHTNESS);
#define STATUS_BT sb.r, sb.g, sb.b
#define STATUS_WIRED sw.r, sw.g, sw.b

    if (keymap_config.no_gui) win_lock_dark(led_min, led_max);
    caps_key(led_min, led_max);
    if ((kb_extra.key_flags & KEY_FLAG_FN_HIGHLIGHT) && IS_LAYER_ON(RK71_FN_LAYER)) fn_highlight(led_min, led_max);

    if (kb_config.side_no_status) {
        if (bt && show_link) {
            switch (link_state) {
                case LINK_PAIRING:
                    if (blink(now, 200)) light(KEY_HOST(kb_config.host), STATUS_BT, led_min, led_max);
                    break;
                case LINK_CONNECTED:
                    light(KEY_HOST(kb_config.host), STATUS_BT, led_min, led_max);
                    break;
                default:
                    if (blink(now, 500)) light(KEY_HOST(kb_config.host), STATUS_BT, led_min, led_max);
                    break;
            }
        }
        if (mode_flash.active && blink(now, 250)) {
            if (bt) {
                light(KEY_P, STATUS_BT, led_min, led_max);
            } else {
                light(KEY_P, STATUS_WIRED, led_min, led_max);
            }
        }
        if (forget_flash.active) light(KEY_BSPC, RGB_RED, led_min, led_max);
        if (low_battery_warning() && blink(now, 1000)) light(KEY_SPACE, RGB_RED, led_min, led_max);
    }

    /* While Fn is held: P shows the mode (green wired, blue Bluetooth), the slot key is blue. */
    if (IS_LAYER_ON(RK71_FN_LAYER)) {
        if (bt) {
            light(KEY_P, STATUS_BT, led_min, led_max);
            light(KEY_HOST(kb_config.host), STATUS_BT, led_min, led_max);
        } else {
            light(KEY_P, STATUS_WIRED, led_min, led_max);
        }
    }
#undef STATUS_BT
#undef STATUS_WIRED
    return true;
}

/* ---- VIA ----------------------------------------------------------------------------------------- */
#ifdef VIA_ENABLE
/* VIA saves the key lighting after every step of a slider or the colour picker, and QMK writes it to
 * flash at once each time: dozens of writes while dragging, each freezing the LEDs on one colour.
 * The save is postponed until the changes settle, like the side-light settings below. */
bool via_command_kb(uint8_t *data, uint8_t length) {
    if (data[0] == id_custom_save && data[1] == id_qmk_rgb_matrix_channel) {
        rgb_save_pending  = true;
        save_requested_at = timer_read32();
        raw_hid_send(data, length); /* acknowledge, as QMK would */
        return true;
    }
    /* The configurator's firmware update: restart into the ROM bootloader (0C45:7040), like
     * Fn+Pause. VIA reserves this command id but QMK leaves it unhandled; the magic word makes sure
     * no other tool's packet can trigger it. Pending settings are saved first. */
    if (data[0] == id_bootloader_jump && length >= 5 && memcmp(&data[1], "RK71", 4) == 0) {
        flush_pending_saves();
        raw_hid_send(data, length);
        wait_ms(50); /* let the reply leave before USB goes away */
        reset_keyboard();
        return true;
    }
    return false;
}

/* "Side lights" settings page */
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
    ID_CAPS_ON_SIDE,
    ID_CAPS_SIDE_COLOUR,
    ID_FROW_ON_SIDE,
    ID_FROW_SIDE_COLOUR,
    ID_CAPS_KEY_MODE,
    ID_FN_HIGHLIGHT,
    ID_SIDE_EFFECT,
    ID_SIDE_FX_SPEED,
    ID_STATUS_WIRED_COLOUR,
    ID_STATUS_BT_COLOUR,
    ID_CAPS_KEY_COLOUR,
    ID_LIGHTS_OFF,
    ID_SLEEP_AFTER,
    ID_AUTO_BATTERY,
    ID_DIM_ON_BATTERY,
    ID_DIM_LEVEL,
    ID_SIDE_CALIBRATE = 28,
    ID_SIDE_TRIM_FIRST,                          /* light 1 red, green, blue, light 2 red, … */
    ID_SIDE_TRIM_LAST = ID_SIDE_TRIM_FIRST + 17, /* light 6 blue */
    ID_PULSE_EXTRA_FIRST,                        /* red, green, blue */
    ID_PULSE_EXTRA_LAST = ID_PULSE_EXTRA_FIRST + 2,
};

static hs_t hs_store(uint8_t h, uint8_t s, hs_t def) {
    return (hs_t){h ^ def.h, s ^ def.s};
}

/* VIA colour pickers send and expect [hue, saturation]. */
static hs_t *colour_setting(uint8_t value_id, hs_t *def) {
    switch (value_id) {
        case ID_SIDE_COLOUR: *def = DEF_SIDE_FIXED; return &kb_extra.side_fixed;
        case ID_CAPS_SIDE_COLOUR: *def = DEF_CAPS_SIDE; return &kb_extra.caps_side;
        case ID_FROW_SIDE_COLOUR: *def = DEF_FROW_SIDE; return &kb_extra.frow_side;
        case ID_CAPS_KEY_COLOUR: *def = DEF_CAPS_KEY; return &kb_extra.caps_key;
        case ID_STATUS_WIRED_COLOUR: *def = DEF_STATUS_WIRED; return &kb_extra.status_wired;
        case ID_STATUS_BT_COLOUR: *def = DEF_STATUS_BT; return &kb_extra.status_bt;
        default: return NULL;
    }
}

static void set_flag(uint8_t *flags, uint8_t flag, bool on) {
    *flags = on ? *flags | flag : *flags & ~flag;
}

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
    hs_t  def;
    hs_t *colour = colour_setting(value_id, &def);
    if (colour && *command_id == id_custom_set_value) {
        *colour = hs_store(value[0], value[1], def);
        return;
    }
    if (colour && *command_id == id_custom_get_value) {
        hs_t c   = hs_value(*colour, def);
        value[0] = c.h;
        value[1] = c.s;
        return;
    }
    if (value_id >= ID_PULSE_EXTRA_FIRST && value_id <= ID_PULSE_EXTRA_LAST) {
        uint8_t c = value_id - ID_PULSE_EXTRA_FIRST;
        if (*command_id == id_custom_set_value) {
            uint8_t v               = value[0] > 63 ? 63 : value[0];
            kb_extra.pulse_extra[c] = v ^ def_pulse_extra[c];
            side_set_pulse_extra(c, v);
            return;
        }
        if (*command_id == id_custom_get_value) {
            value[0] = PULSE_EXTRA_V(c);
            return;
        }
    }
    if (value_id >= ID_SIDE_TRIM_FIRST && value_id <= ID_SIDE_TRIM_LAST) {
        uint8_t i = value_id - ID_SIDE_TRIM_FIRST, p = i / 3, c = i % 3;
        if (*command_id == id_custom_set_value) {
            kb_extra.side_trim[p][c] = (value[0] & 31) ^ 31;
            side_set_trim(p, c, value[0] & 31);
            return;
        }
        if (*command_id == id_custom_get_value) {
            value[0] = kb_extra.side_trim[p][c] ^ 31;
            return;
        }
    }
    switch (*command_id) {
        case id_custom_set_value:
            switch (value_id) {
                case ID_SIDE_MODE: kb_config.side_mode = value[0] > SIDE_MODE_OFF ? SIDE_MODE_FOLLOW : value[0]; break;
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
                case ID_CAPS_ON_SIDE: set_flag(&kb_extra.side_flags, SIDE_FLAG_CAPS, value[0]); break;
                case ID_FROW_ON_SIDE: set_flag(&kb_extra.side_flags, SIDE_FLAG_FROW, value[0]); break;
                case ID_CAPS_KEY_MODE: kb_extra.caps_key_mode = value[0] <= CAPS_KEY_CUSTOM ? value[0] : CAPS_KEY_AUTO; break;
                case ID_SIDE_EFFECT: kb_extra.side_effect = value[0] < SIDE_FX_COUNT ? value[0] : SIDE_FX_STATIC; break;
                case ID_SIDE_FX_SPEED: kb_extra.side_fx_speed = (value[0] & 31) ^ DEF_FX_SPEED; break;
                case ID_LIGHTS_OFF: kb_extra.lights_off = (value[0] % TIME_CHOICES) ^ DEF_LIGHTS_OFF; break;
                case ID_SLEEP_AFTER: kb_extra.sleep_after = (value[0] % TIME_CHOICES) ^ DEF_SLEEP_AFTER; break;
                case ID_AUTO_BATTERY: set_flag(&kb_extra.power_flags, POWER_FLAG_NO_AUTO_BATTERY, !value[0]); break;
                case ID_DIM_ON_BATTERY: set_flag(&kb_extra.power_flags, POWER_FLAG_DIM_ON_BATTERY, value[0]); break;
                case ID_DIM_LEVEL: kb_extra.dim_level = (value[0] & 31) ^ DEF_DIM_LEVEL; break;
                case ID_SIDE_CALIBRATE: side_calibrating = value[0]; break;
                case ID_FN_HIGHLIGHT: set_flag(&kb_extra.key_flags, KEY_FLAG_FN_HIGHLIGHT, value[0]); break;
                default: *command_id = id_unhandled; break;
            }
            side_set_tuning(SIDE_RED_V, SIDE_GREEN_V, SIDE_BLUE_V);
            break;
        case id_custom_get_value:
            switch (value_id) {
                case ID_SIDE_MODE: value[0] = kb_config.side_mode; break;
                case ID_SIDE_STATUS: value[0] = !kb_config.side_no_status; break;
                case ID_SIDE_RED: value[0] = SIDE_RED_V; break;
                case ID_SIDE_GREEN: value[0] = SIDE_GREEN_V; break;
                case ID_SIDE_BLUE: value[0] = SIDE_BLUE_V; break;
                case ID_SIDE_BRIGHT: value[0] = SIDE_BRIGHT_V; break;
                case ID_SIDE_FOLLOW_BRIGHTNESS: value[0] = !(kb_extra.side_flags & SIDE_FLAG_OWN_BRIGHTNESS); break;
                case ID_STATUS_BRIGHT: value[0] = STATUS_BRIGHT_V; break;
                case ID_CAPS_ON_SIDE: value[0] = !!(kb_extra.side_flags & SIDE_FLAG_CAPS); break;
                case ID_FROW_ON_SIDE: value[0] = !!(kb_extra.side_flags & SIDE_FLAG_FROW); break;
                case ID_CAPS_KEY_MODE: value[0] = kb_extra.caps_key_mode; break;
                case ID_SIDE_EFFECT: value[0] = kb_extra.side_effect; break;
                case ID_SIDE_FX_SPEED: value[0] = FX_SPEED_V; break;
                case ID_LIGHTS_OFF: value[0] = LIGHTS_OFF_V; break;
                case ID_SLEEP_AFTER: value[0] = SLEEP_AFTER_V; break;
                case ID_AUTO_BATTERY: value[0] = !(kb_extra.power_flags & POWER_FLAG_NO_AUTO_BATTERY); break;
                case ID_DIM_ON_BATTERY: value[0] = !!(kb_extra.power_flags & POWER_FLAG_DIM_ON_BATTERY); break;
                case ID_DIM_LEVEL: value[0] = DIM_LEVEL_V; break;
                case ID_SIDE_CALIBRATE: value[0] = side_calibrating; break;
                case ID_FN_HIGHLIGHT: value[0] = !!(kb_extra.key_flags & KEY_FLAG_FN_HIGHLIGHT); break;
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
