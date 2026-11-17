/* Copyright 2024 Dimitris Mantzouranis <d3xter93@gmail.com>
 * Copyright 2026 josemanuque (RK71 fast row switching)
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 2 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <http://www.gnu.org/licenses/>.
 */

/*
 * RK71 version of SonixQMK's drivers/led/sn32f2xx.c (commit c9f3803f). QMK compiles this file
 * instead of the stock one because the keyboard folder comes first in the source lookup (VPATH);
 * the public functions and the shared key matrix behave the same.
 *
 * How the key LEDs are driven: 15 "colour rows" (5 key rows x R/B/G anode pins) take turns. Each turn
 * the 16 column cathodes are hardware PWM channels of CT16B1 whose duty is that row's colour values,
 * for one PWM period. At the end of the period this interrupt switches to the next colour row and
 * restarts the timer. The LEDs are dark while it runs, so its length is added to every row: it sets
 * how bright the keys can be and how often they refresh.
 *
 * Why this copy exists: the stock driver stops the row timer at the end of each row and restarts it
 * only after its interrupt has switched rows, so the whole interrupt (measured on the RK71: 137 us
 * with the stock code, 32 us after slimming it) is added to every row as darkness. At the 50 us rows
 * needed for a flicker-free refresh (IEEE 1789: 1250 Hz or more at full modulation) that made the keys
 * refresh at 357 Hz instead of 1333 Hz and light only 27% of the time.
 *
 * Here the switch is as short as it can be: one pin off, 16 register writes from values prepared on
 * flush, one pin on. The timer still waits for it (restarted at the end of the interrupt): letting it
 * run on by itself was tried and flickers, because the interrupt's start time jitters (other
 * interrupts, mainly the side-light timer, delay it) and a row's on-time then varies from cycle to
 * cycle. Only the configuration this board uses is kept: SN32F240B, COL2ROW, hardware PWM, columns
 * shared with the key matrix.
 */

#include <string.h>
#include "matrix.h"
#include "rgb_matrix.h"
#include "sn32f2xx.h"

/* Same configuration names as the stock driver. */
#if !defined(MATRIX_IO_DELAY)
#    define MATRIX_IO_DELAY 30
#endif
#define SN32F2XX_PWM_OUTPUT_ACTIVE_HIGH PWM_OUTPUT_ACTIVE_HIGH
#define SN32F2XX_PWM_OUTPUT_ACTIVE_LOW PWM_OUTPUT_ACTIVE_LOW
#define SN32F2XX_RGB_OUTPUT_ACTIVE_HIGH PWM_OUTPUT_ACTIVE_HIGH
#define SN32F2XX_RGB_OUTPUT_ACTIVE_LOW PWM_OUTPUT_ACTIVE_LOW
#define HARDWARE_PWM 0
#define SOFTWARE_PWM 1

#if !defined(SN32F240B) || (SN32F2XX_PWM_DIRECTION != COL2ROW) || (SN32F2XX_PWM_CONTROL != HARDWARE_PWM) || !defined(SHARED_MATRIX)
#    error This RK71 driver only implements SN32F240B, COL2ROW, hardware PWM with a shared matrix
#endif

#if (SN32F2XX_RGB_OUTPUT_ACTIVE_LEVEL != SN32F2XX_RGB_OUTPUT_ACTIVE_HIGH)
#    error The RK71 row (anode) pins are active high
#endif

/* Timer and period, as the stock driver derives them (see config.h for the chosen refresh rate). */
static const uint32_t periodticks = RGB_MATRIX_MAXIMUM_BRIGHTNESS;
static const uint32_t freq        = (RGB_MATRIX_HUE_STEP * RGB_MATRIX_SAT_STEP * RGB_MATRIX_VAL_STEP * RGB_MATRIX_SPD_STEP * RGB_MATRIX_LED_PROCESS_LIMIT);

static const pin_t led_row_pins[SN32F2XX_RGB_MATRIX_ROWS_HW] = SN32F2XX_RGB_MATRIX_ROW_PINS; /* R, B, G per key row */
static const pin_t led_col_pins[SN32F2XX_RGB_MATRIX_COLS]    = SN32F2XX_RGB_MATRIX_COL_PINS;

static uint8_t  chan_col_order[SN32F2XX_RGB_MATRIX_COLS]; /* PWM channel of each column */
static uint32_t col_pwm_io_mask;                          /* PWMIOENB bits of all column channels */

static RGB led_state_buf[SN32F2XX_LED_COUNT]; /* what rgb_matrix last set */
bool       led_state_buf_update_required = false;

/* What the interrupt shows, prepared on flush so a row switch is only register writes: each colour
 * row's 16 duty cycles, and whether anything in it is lit. */
static uint8_t             row_duty[SN32F2XX_RGB_MATRIX_ROWS_HW][SN32F2XX_RGB_MATRIX_COLS];
static bool                row_lit[SN32F2XX_RGB_MATRIX_ROWS_HW];
static volatile uint32_t  *col_duty_reg[SN32F2XX_RGB_MATRIX_COLS]; /* match register of each column */

static uint8_t current_row     = 0;    /* colour row being shown */
static uint8_t current_key_row = 0;    /* its key row */
static uint8_t lit_row         = 0xFF; /* row pin currently high, if any */

/* Shared key matrix: the columns are read between LED rows. */
extern matrix_row_t  raw_matrix[MATRIX_ROWS];
static matrix_row_t  shared_matrix[MATRIX_ROWS];
static volatile bool matrix_locked  = false;
static volatile bool matrix_scanned = false;

/* Measurements for the configurator (VIA values 50/51, read through rk71_rgb.c). */
volatile uint32_t sn32f2xx_rows_lit;   /* colour rows shown since start-up */
volatile uint32_t sn32f2xx_wait_sum;   /* timer ticks from the end of a row until this code ran (summed) */
volatile uint32_t sn32f2xx_switch_sum; /* timer ticks from the end of a row until the next one started */
volatile uint16_t sn32f2xx_switch_max; /* longest of those since last read */

static void rgb_callback(PWMDriver *pwmp);

static PWMConfig pwmcfg = {
    freq,
    periodticks,
    NULL,
    {
        [0 ... PWM_CHANNELS - 1] = {PWM_OUTPUT_DISABLED, NULL, 0},
    },
    0,
};

void matrix_output_unselect_delay(uint8_t line, bool key_pressed) {
    for (int i = 0; i < TIME_US2I(MATRIX_IO_DELAY); ++i) {
        __asm__ volatile("" ::: "memory");
    }
}

bool matrix_can_read(void) {
    return matrix_scanned;
}

/* Which PWM channel drives each column pin (per the SN32F240B pin function table:
 * channel = (port * 16 + pin) % 24, alternate function when that number is above 23). */
static void rgb_ch_ctrl(PWMConfig *cfg) {
    for (uint8_t i = 0; i < SN32F2XX_RGB_MATRIX_COLS; i++) {
        if (led_col_pins[i] > C15) continue; /* only P0.0 to P2.15 can be PWM outputs */
        uint8_t pio_value = ((uint32_t)(PAL_PORT(led_col_pins[i])) - (uint32_t)(PAL_PORT(A0))) / ((uint32_t)(PAL_PORT(B0)) - (uint32_t)(PAL_PORT(A0))) * PAL_IOPORTS_WIDTH + PAL_PAD(led_col_pins[i]);
        uint8_t ch_idx    = pio_value % PWM_CHANNELS;
        chan_col_order[i] = ch_idx;
        col_duty_reg[i]   = &SN32_CT16B1->MR[ch_idx];
        cfg->channels[ch_idx].pfpamsk = pio_value > (PWM_CHANNELS - 1);
        cfg->channels[ch_idx].mode    = SN32F2XX_PWM_OUTPUT_ACTIVE_LEVEL;
        col_pwm_io_mask |= mskCT16_PWMnIOEN_EN(ch_idx);
    }
}

/* Read one key row while the columns are plain inputs (same scheme as the stock driver: a full
 * pass over the rows, then the main loop takes the result through matrix_scan_custom). */
static void scan_keys(uint8_t current_key, uint8_t last_key) {
    static uint8_t first_scanned;
    if (!matrix_locked) {
        matrix_locked = true;
        first_scanned = current_key;
    } else if ((last_key != current_key) && (current_key == first_scanned)) {
        matrix_locked = false;
    }
    if (matrix_locked) {
        matrix_read_cols_on_row(shared_matrix, current_key);
        matrix_scanned = true;
    }
}

static void rgb_callback(PWMDriver *pwmp) {
    /* The counter runs on past the end of the row (periodticks) until restarted below, so the excess
     * is time spent between rows. */
    uint16_t entered = SN32_CT16B1->config.TC - periodticks;
    sn32f2xx_rows_lit++;
    sn32f2xx_wait_sum += entered;

    /* Dark first: only one row pin is ever high. */
    if (lit_row != 0xFF) {
        gpio_write_pin_low(led_row_pins[lit_row]);
        lit_row = 0xFF;
    }

    if (EFLD1.state != FLASH_PGM) { /* no LED or matrix work while flash is being written */
        uint8_t last_key_row = current_key_row;
        if (++current_row == SN32F2XX_RGB_MATRIX_ROWS_HW) current_row = 0;
        if (current_row % SN32F2XX_RGB_MATRIX_ROW_CHANNELS == 2) current_key_row++;
        if (current_key_row == SN32F2XX_RGB_MATRIX_ROWS) current_key_row = 0;

        /* Key scan, once per key row (when it changes, every third colour row): hand the column pins
         * back to the GPIO block for the read. A full pass still takes only ~1 ms. */
        bool scanned = !matrix_scanned && current_key_row != last_key_row;
        if (scanned) {
            SN32_CT16B1->pwm.PWMIOENB &= ~col_pwm_io_mask;
            scan_keys(current_key_row, last_key_row);
        }

        /* Next row's duty cycles, prepared in sn32f2xx_flush(), straight into the match registers. */
        const uint8_t *duty = row_duty[current_row];
        for (uint8_t col = 0; col < SN32F2XX_RGB_MATRIX_COLS; col++) {
            *col_duty_reg[col] = duty[col];
        }
        if (scanned) SN32_CT16B1->pwm.PWMIOENB |= col_pwm_io_mask; /* columns back to the PWM */

        if (row_lit[current_row]) {
            gpio_write_pin_high(led_row_pins[current_row]);
            lit_row = current_row;
        }
    }

    uint16_t took = SN32_CT16B1->config.TC - periodticks; /* whole gap between rows */
    sn32f2xx_switch_sum += took;
    if (took > sn32f2xx_switch_max) sn32f2xx_switch_max = took;

    pwm_lld_change_counter(pwmp, UINT16_MAX); /* wraps on the next tick: the new row starts now */
}

/* The timer's interrupt, handled here directly (SN32_CT16B1_SUPPRESS_ISR in mcuconf.h): the only
 * source enabled on CT16B1 is the end-of-row match. */
OSAL_IRQ_HANDLER(SN32_CT16B1_HANDLER) {
    OSAL_IRQ_PROLOGUE();
    uint32_t ris          = SN32_CT16B1->irq.RIS;
    SN32_CT16B1->irq.IC   = ris;
    if (ris & mskCT16_MRnIF(PWM_CHANNELS)) rgb_callback(&PWMD1);
    OSAL_IRQ_EPILOGUE();
}

void sn32f2xx_init(void) {
    for (uint8_t x = 0; x < SN32F2XX_RGB_MATRIX_ROWS_HW; x++) {
        gpio_set_pin_output_push_pull(led_row_pins[x]);
        gpio_write_pin_low(led_row_pins[x]);
    }
    rgb_ch_ctrl(&pwmcfg);
    memset(shared_matrix, 0, sizeof(shared_matrix));
    pwmStart(&PWMD1, &pwmcfg);
    nvicEnableVector(SN32_CT16B1_NUMBER, SN32_PWM_CT16B1_IRQ_PRIORITY); /* ChibiOS leaves this to us */
    pwmcfg.callback = rgb_callback;
    pwmEnablePeriodicNotification(&PWMD1);
}

/* Turn the LED colours into per-row duty cycles. Colour row r is key row (r + 1) / 3 % rows (the
 * stock driver's order, which matches the board's R, B, G anode pins) showing channel r % 3:
 * 0 = blue, 1 = green, 2 = red. */
void sn32f2xx_flush(void) {
    if (!led_state_buf_update_required) return;
    led_state_buf_update_required = false;
    for (uint8_t row = 0; row < SN32F2XX_RGB_MATRIX_ROWS_HW; row++) {
        uint8_t key_row = ((row + 1) / SN32F2XX_RGB_MATRIX_ROW_CHANNELS) % SN32F2XX_RGB_MATRIX_ROWS;
        uint8_t channel = row % SN32F2XX_RGB_MATRIX_ROW_CHANNELS;
        bool    lit     = false;
        for (uint8_t col = 0; col < SN32F2XX_RGB_MATRIX_COLS; col++) {
            uint8_t led_index = g_led_config.matrix_co[key_row][col];
            uint8_t duty      = 0;
            if (led_index < SN32F2XX_LED_COUNT) { /* NO_LED where a key position has no LED */
                const RGB *led = &led_state_buf[led_index];
                duty           = channel == 0 ? led->b : channel == 1 ? led->g : led->r;
                lit |= (led->r | led->g | led->b) != 0;
            }
            row_duty[row][col] = duty;
        }
        row_lit[row] = lit;
    }
}

void sn32f2xx_set_color(int index, uint8_t r, uint8_t g, uint8_t b) {
    uint8_t color_r = r * SN32F2XX_LED_OUTPUT_LUMINOSITY_R;
    uint8_t color_g = g * SN32F2XX_LED_OUTPUT_LUMINOSITY_G;
    uint8_t color_b = b * SN32F2XX_LED_OUTPUT_LUMINOSITY_B;
    if (led_state_buf[index].r == color_r && led_state_buf[index].b == color_b && led_state_buf[index].g == color_g) {
        return;
    }
    led_state_buf[index].r        = color_r;
    led_state_buf[index].b        = color_b;
    led_state_buf[index].g        = color_g;
    led_state_buf_update_required = true;
}

void sn32f2xx_set_color_all(uint8_t r, uint8_t g, uint8_t b) {
    for (int i = 0; i < SN32F2XX_LED_COUNT; i++) {
        sn32f2xx_set_color(i, r, g, b);
    }
}

bool matrix_scan_custom(matrix_row_t current_matrix[]) {
    if (!matrix_scanned) return false; /* nothing until a full pass has been read */

    bool changed = memcmp(raw_matrix, shared_matrix, sizeof(shared_matrix)) != 0;
    if (changed) memcpy(raw_matrix, shared_matrix, sizeof(shared_matrix));

    matrix_scanned = false;
    return changed;
}
