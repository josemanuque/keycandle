# RK71 Bluetooth link — SN32 ↔ BT module

Reverse-engineered from the stock firmware `rk71_fw.bin` (V1.12) on 2026-11-06, using
static disassembly only. Register meanings were checked against the SN32F240B datasheet v2.4.
Since v0.4 the QMK port drives the module with exactly this protocol (`keyboard/rk71_rgb/rk71_bt.c`), which
confirms everything below on the live board except what's listed under "Still unknown" at the bottom.

## Physical link

| Item | Value | Evidence |
|---|---|---|
| Bus | **SPI0**, not UART. The firmware never touches UART0/1/2 or I2C0. | Only SPI0 (0x4001C000) is referenced. |
| Role | **SN32 = SPI slave**, BT module = master (drives SCK) | init @0x68B0: CTRL0.MS = 1 |
| Format | 8-bit frames, mode 0 (CPOL=0, CPHA=0), MSB first, hardware SEL | CTRL0.DL = 7, CTRL1 = 0 |
| Pins (datasheet) | P0.2 MISO0, P0.3 MOSI0, P0.4 SCK0, P0.5 SEL0 | LQFP64 pin table |
| RX handling | IRQ 6 (SPI0), RX-FIFO-threshold interrupt | handler @0x5866 |
| **P0.0** (output) | SN32 → module "I have a packet" request. Set high to start a send, cleared when done or on timeout. | BSET/BCLR on GPIO0 bit 0 |
| **P0.1** (input) | Direction. High = module is sending to SN32; low = SN32 may send. | ISR branches on GPIO0 bit 1 |

GPIO register offsets (datasheet): DATA 0x00, MODE 0x04, BSET **0x24**, BCLR **0x28**.
(ANALYSIS.md lists BSET 0x20 / BCLR 0x24 / TOGGLE 0x28, which is wrong.)

## Send sequence (SN32 → module)

1. Proceed only if P0.1 is low and the previous send has finished (`0x200001CF` = 1).
2. Set P0.0 high.
3. Fill the TX buffer (`0x20000368`, one halfword per byte) with the header and payload, and set the length (`0x200001C3`).
4. Wait for SPI not busy, reset the SPI FIFO, preload the first byte into SPI DATA.
5. The master clocks the bytes out. The ISR feeds the next byte on each RX interrupt.
6. After the last byte, the ISR clears P0.0 and marks the send as done.
7. Timeout: a 200-tick countdown (`0x200001D0`, routine @0x5816) forces the send to finish.

## Packets SN32 → module

| Header | Payload | Meaning (confirmed) | Source buffer | Builder |
|---|---|---|---|---|
| `A1` | 8 bytes | Boot keyboard report: mods, reserved, 6 keys | 0x2000015E | @0x5180 |
| `A2` | 15 bytes | NKRO key bitmap (120 bits) | 0x20000318 | @0x520E |
| `A3` | 2 bytes | Consumer usage, 16-bit (media keys) | 0x20000166 | @0x529A |
| `A4` | 1 byte | System control (power/sleep) | 0x20000169 | @0x531C |
| `A6` | 2 bytes | Control command (see below) | 0x200001C1 | several |

### `A6` control commands

| Bytes | Sent from | Meaning (confirmed, see the ITON table below) |
|---|---|---|
| `51 81` … `51 85` | 0x56CA–0x571A, and key handlers @0x2948, 0x37DA… | Select BT host/channel 1–5. The value is picked from bitmask `0x20000175` (1, 2, 4, 8, 16). |
| `51 89` | @0x04A8, @0x0646 | Enter pairing on the current slot. |
| `51 70` | @0x57A2 | Forget all pairings. |
| `58 01` | @0x5638 | Wired (USB) mode: the module goes idle. |

## Packets module → SN32 (parser @0x5410)

| Bytes | Effect in firmware | Meaning (confirmed) |
|---|---|---|
| `B1 xx` | Low 3 bits stored to `0x200002B0` | Host LED state (Num/Caps/Scroll Lock) |
| `B6 5A 06` | Starts timers (600 / 300 ticks), calls 0x5138 | Status event, e.g. pairing started |
| `B6 5A 07` | Sets flag `0x20000240` = 1, timer 50 | Status event, e.g. disconnected |
| `B6 5A 0A` | Clears `0x200001DC`, `0x200001E4`, `0x20000240` | Status event, e.g. connected |
| `B6 51 76` / `77` / `78` | Clear pending-command flags | Acknowledgements of `A6 51 xx` commands |

## Update: it is an ITON module, and how the stock keys drive it

Further decoding (2026-11-06) showed the module speaks the same protocol as QMK's `iton_bt` driver
(`drivers/bluetooth/iton_bt.h`), which independently confirms the table above:

| Bytes | Meaning | Stock trigger |
|---|---|---|
| `A6 51 81`…`85` | Select device slot 1–5 and connect | Fn+Q/W/E/R/T (Fn table @0x7942: Q→F3, W→F4, E→F5, R→AD, T→F6) |
| `A6 51 89` | Enter pairing on the current slot | Fn+P held ~3 s (code F1; up to 3 retries) |
| `A6 51 70` | Forget all pairings | Factory-reset routine @0xAE8C |
| `A6 58 01` | Wired (USB) mode, module idle | Fn+P tap while in Bluetooth mode |
| `B6 51 76/77/78/79` | Connected / pairing / disconnected / connecting | |
| `B6 5A 06/07/0A` | Battery low / low-power shutdown / battery OK | |

- The slide switch is not read by the firmware. It only connects the battery. Wired vs Bluetooth is a
  firmware mode (RAM `0x200001E0`, saved at flash `0x8009`), toggled with Fn+P. Entering Bluetooth also
  disables the USB pull-up (@0x0A94).
- Pin setup (@0x653C): A6 and A7 are outputs held low (assumed to be module control lines). Other
  unexplained pins: C0/C1/C3 outputs low; C15, D0–D2, D12–D15 inputs.
- The stock firmware uses no pin interrupt for the link. The SPI0 interrupt checks A1 to tell receive from
  send, and the main loop parses complete packets by length.

## QMK implementation

`keyboard/rk71_rgb/rk71_bt.c` is a register-level port of that stock logic (SPI0 slave, A0/A1
handshake, 20 ms send timeout). It does not use QMK's `iton_bt.c`, which needs pin-change callbacks
(512 bytes of RAM that this 8 KB chip does not have) and blocks forever if the module is unpowered.
`rk71_rgb.c` provides the modes, the Fn keys and the status lights.

## What this means for QMK

The protocol is small, which makes a QMK Bluetooth driver realistic:
- an SPI0-slave driver with the IRQ 6 handler
- the P0.0/P0.1 handshake
- `A1`/`A3`/`A4` packets mapped to QMK's keyboard, consumer and system reports
- `B1` parsed for the Caps Lock LED
- `A6 51 8x` for host switching

## Battery

The module reports the battery as `B6 5A 04` / `02` / `01` (high / medium / low), plus `06` (low), `07` (shutting
down) and `0A` (OK again). That is all: no voltage or percentage. The SN32 chip has no battery wire either (the
stock firmware never touches its ADC), so a percentage would need a hardware change.

## What the computer sees over Bluetooth (tested on Windows, 2026-11-17)

* Classic Bluetooth HID (profile `0x1124`), reported as `05AC:024F` (the module's own ID, not the QMK one).
* Seven HID collections: 2 keyboards, 4 consumer (media keys), 1 system control. No vendor-defined collection
  and no feature reports, so VIA's channel has no Bluetooth equivalent.
* The only computer-to-keyboard data is the lock-LED output report (forwarded to the SN32 as `B1 xx`):
  * `HidD_SetOutputReport` from an app: refused (Windows keeps keyboards for itself).
  * `IOCTL_KEYBOARD_SET_INDICATORS` on the keyboard's own interface (`GUID_DEVINTERFACE_KEYBOARD`): **works**,
    no admin rights, only that keyboard's LEDs change (not the Caps/Num/Scroll lock state). This is the basis
    for one-way settings over Bluetooth in v0.9.
* The media and system collections can be opened and read by apps, a possible keyboard-to-computer path (untested).
* Firmware updates over Bluetooth: no. The ROM bootloader is USB-only.

## Still unknown

- How the module is powered or reset (A6/A7 are held low as on stock; their role isn't traced).
- Whether P0.15 (configured as an input) is involved.
- Whether the module forwards more than the 3 lock bits of `B1 xx` (Compose, Kana).
