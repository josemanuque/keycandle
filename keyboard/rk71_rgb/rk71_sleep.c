// Copyright 2026 josemanuque
// SPDX-License-Identifier: GPL-2.0-or-later
#include "rk71_sleep.h"
#include <hal.h>
#include "gpio.h"
#include "config.h"

#define PMU_DEEP_SLEEP 2 /* PMU_CTRL.MODE, as written by the stock firmware before its WFI */
#define PMU_NORMAL 0
#define WAKE_IRQ_PRIORITY 3

static const pin_t row_pins[]     = MATRIX_ROW_PINS;
static const pin_t col_pins[]     = MATRIX_COL_PINS;
static const pin_t led_row_pins[] = SN32F2XX_RGB_MATRIX_ROW_PINS;

#define VBUS_PIN C15 /* high while the USB cable supplies power (see rk71_side.c) */
/* RK71_BT_DIR_PIN (A1) goes high when the Bluetooth module is about to send (host Caps Lock,
 * connection and battery events). Waking on it keeps those from being lost while asleep. */

#define LEN(a) (sizeof(a) / sizeof((a)[0]))

/* ---- wake-up interrupts ----------------------------------------------------------------------
 * PAL callbacks are disabled (they cost RAM), so the port interrupt vectors are free. The handlers
 * only disarm the interrupt: waking the core is all they are for. */

static void arm_wake(pin_t pin) {
    ioportid_t port = PAL_PORT(pin);
    uint32_t   bit  = 1u << PAL_PAD(pin);
    port->IS &= ~bit; /* edge, not level */
    port->IBS |= bit; /* both edges: no need to know the pin's polarity */
    port->IC = bit;
    port->IE |= bit;
}

static void disarm_port(ioportid_t port) {
    port->IE = 0;
    port->IC = 0xFFFF;
}

OSAL_IRQ_HANDLER(SN32_GPIOA_HANDLER) {
    OSAL_IRQ_PROLOGUE();
    disarm_port(GPIOA);
    OSAL_IRQ_EPILOGUE();
}

OSAL_IRQ_HANDLER(SN32_GPIOB_HANDLER) {
    OSAL_IRQ_PROLOGUE();
    disarm_port(GPIOB);
    OSAL_IRQ_EPILOGUE();
}

OSAL_IRQ_HANDLER(SN32_GPIOC_HANDLER) {
    OSAL_IRQ_PROLOGUE();
    disarm_port(GPIOC);
    OSAL_IRQ_EPILOGUE();
}

void rk71_sleep_init(void) {
    disarm_port(GPIOA);
    disarm_port(GPIOB);
    disarm_port(GPIOC);
    nvicEnableVector(SN32_GPIOA_NUMBER, WAKE_IRQ_PRIORITY);
    nvicEnableVector(SN32_GPIOB_NUMBER, WAKE_IRQ_PRIORITY);
    nvicEnableVector(SN32_GPIOC_NUMBER, WAKE_IRQ_PRIORITY);
}

/* A key is down (its column reads low), the cable just came in, or the module is talking. */
static bool wake_condition(void) {
    for (uint8_t i = 0; i < LEN(col_pins); i++) {
        if (!gpio_read_pin(col_pins[i])) return true;
    }
    return gpio_read_pin(VBUS_PIN) || gpio_read_pin(RK71_BT_DIR_PIN);
}

/* Stop the LED PWM / key-scan interrupt (sn32f2xx driver), hand the column pins back from the PWM
 * unit to GPIO, and switch every LED row off (active high). */
void rk71_leds_pause(void) {
    pwmDisablePeriodicNotification(&PWMD1);
    for (uint8_t ch = 0; ch < PWM_CHANNELS; ch++) pwmDisableChannel(&PWMD1, ch);
    for (uint8_t i = 0; i < LEN(led_row_pins); i++) gpio_write_pin_low(led_row_pins[i]);
}

void rk71_leds_resume(void) {
    pwmEnablePeriodicNotification(&PWMD1); /* the next scan step re-enables the columns */
}

void rk71_sleep(void) {
    /* 1. LEDs and key scan off. */
    rk71_leds_pause();

    /* 2. Matrix in wake-up state: all rows driven low, columns pulled up. A pressed key pulls
     *    its column low through its diode. */
    for (uint8_t i = 0; i < LEN(row_pins); i++) {
        gpio_set_pin_output(row_pins[i]);
        gpio_write_pin_low(row_pins[i]);
    }
    for (uint8_t i = 0; i < LEN(col_pins); i++) {
        gpio_set_pin_input_high(col_pins[i]);
        arm_wake(col_pins[i]);
    }
    gpio_set_pin_input(VBUS_PIN);
    arm_wake(VBUS_PIN);
    arm_wake(RK71_BT_DIR_PIN);

    /* 3. Only the wake-up ports may wake the core; everything else (USB, SPI, timers) stays
     *    pending and is served after waking. */
    uint32_t enabled   = NVIC->ISER[0];
    uint32_t wake_irqs = (1u << SN32_GPIOA_NUMBER) | (1u << SN32_GPIOB_NUMBER) | (1u << SN32_GPIOC_NUMBER);
    NVIC->ICER[0]      = enabled & ~wake_irqs;

    /* 4. Sleep. Interrupts are masked around the check so a key pressed in between cannot be
     *    missed: WFI still wakes on a pending interrupt while PRIMASK is set. */
    __disable_irq();
    if (!wake_condition()) {
        SN_PMU->CTRL = PMU_DEEP_SLEEP;
        __WFI();
        SN_PMU->CTRL = PMU_NORMAL;
    }
    __enable_irq(); /* the port handler runs here and disarms its interrupt */

    /* 5. Back to normal: no wake-up interrupts, rows released as the QMK matrix expects
     *    (unselected = input with pull-up), scanning restarted. */
    disarm_port(GPIOA);
    disarm_port(GPIOB);
    disarm_port(GPIOC);
    NVIC->ISER[0] = enabled;
    for (uint8_t i = 0; i < LEN(row_pins); i++) gpio_set_pin_input_high(row_pins[i]);
    rk71_leds_resume();
}
