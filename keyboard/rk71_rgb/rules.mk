# Bluetooth through the on-board ITON module. rk71_bt.c is a small SPI0-slave driver that
# works the way the stock firmware does (no pin-change interrupts, which would cost 512 bytes
# of RAM this 8 KB chip does not have); rk71_rgb.c holds the modes, keys and status lights.
BLUETOOTH_ENABLE = yes
BLUETOOTH_DRIVER = custom
SRC += rk71_bt.c rk71_side.c rk71_sleep.c rk71_rand.c rk71_keyled.c rk71_btcfg.c

# Key LED brightness scaling (dim on battery): calls into the SN32 LED driver go through
# rk71_keyled.c first.
EXTRALDFLAGS += -Wl,--wrap=sn32f2xx_set_color -Wl,--wrap=sn32f2xx_set_color_all

# Per-key debounce: a bouncy switch (seen as double letters) is filtered without slowing other keys.
DEBOUNCE_TYPE = sym_defer_pk

# Settings storage: QMK's own code (copied into nvm_*.c here), except that macros get 2 KB of their
# own flash, read in place (nvm_dynamic_keymap.c). The emulated EEPROM is mirrored in RAM.
NVM_DRIVER = custom
SRC += nvm_dynamic_keymap.c nvm_via.c
