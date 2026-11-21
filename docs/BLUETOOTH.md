# RK71 Bluetooth: the module, and settings over Bluetooth

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
    no admin rights, only that keyboard's LEDs change (not the Caps/Num/Scroll lock state). This carries
    [settings over Bluetooth](#settings-over-bluetooth).
* The media and system collections can be opened and read by apps: that's the way back from the keyboard.
* Firmware updates over Bluetooth: no. The ROM bootloader is USB-only.

## Settings over Bluetooth

Since v0.9 the app changes settings over Bluetooth and hears the keyboard's own changes. The module offers the
computer only keyboard and media interfaces, so both directions borrow something that already exists.
Firmware: `keyboard/rk71_rgb/rk71_btcfg.c`. App: `app/src/btlink.rs`.

### Computer → keyboard: the lock lights

Every computer can set a keyboard's Num, Caps and Scroll Lock lights, and the module forwards each change to the
chip as `B1 xx`. The app sets them directly, without touching the real lock states:
* **Windows:** `IOCTL_KEYBOARD_SET_INDICATORS` on the keyboard's own interface, no admin rights.
* **macOS:** an output report (needs Input Monitoring).
* **Linux:** hidraw (the udev rule).

* **Symbol:** one light state, held 30 ms. Scroll flips on every symbol (the clock); Num and Caps carry 2 bits.
* **Message:** 4 start symbols (data 1, 2, 1, 2), then 5 bytes at 4 symbols each, high bits first:
  1. channel;
  2. value id;
  3. value byte 0;
  4. value byte 1;
  5. CRC-8 (poly 0x07) of the first four.

  24 symbols in all, about 0.72 s.
* **Dropped:** a gap over 250 ms, a bad start or a bad CRC drops the message, so a half-received setting is never
  applied.
* **Confirmed:** an intact message is applied and confirmed with the media usage `0x3F0 | (CRC & 0x0F)`, then a
  release. The app waits 0.7 s for it and sends again, up to 4 times. Applying a message twice is harmless.

A lock key pressed during a message breaks it, and the app resends. Ordinary keys don't touch the lights. The Caps
key's own light ignores changes shorter than 200 ms, so it doesn't flicker. While messages arrive, the keyboard
puts off saving to flash for 1.5 s, because a flash write stalls the chip long enough to lose an LED report.

### Keyboard → computer: media codes nobody uses

Values go back as consumer (media-key) usages in a reserved range that operating systems ignore. Each value is
two usages, then a release (before v0.10.1, a release after each). The module sends about one report every 90 ms,
so every report saved counts:

```
0x300 | slot << 4 | high 4 bits,   then   0x380 + (slot << 4) | low 4 bits        (slot 0..6)
```

| Slot | Value |
|---|---|
| 0–4 | Key brightness, effect, speed, hue, saturation |
| 5 | Windows or Mac per device |
| 6 | The keyboard's state: device slot (bits 0–2), battery 0–3 (bits 3–4), battery low (bit 5), Win-key lock (bit 6) |

The app reads them from the keyboard's media collection (report 8) and pairs up the halves.

### The flow

```
App                                     Keyboard                         BT module
 |-- "send what changed" + CRC --------->|  ~0.72 s (lock lights)           |
 |<-- confirmation 0x3F0 + CRC ----------|-- "battery level?" ------------->|
 |                                       |<-- high / medium / low ----------|
 |                                       |   0.25 s after the last symbol:
 |<-- slot 6: battery, device -----------|   the state first
 |<-- the other values, if they differ --|   brightness, effect, … Win/Mac
```

* **When the app finds the keyboard:** it sends the request above on channel `0xF0`, id 0. Its value bytes are
  `1` and a CRC-8 of the values the app remembers from the cable (slots 0–5). The keyboard computes the same CRC
  over its own values: if they match, it sends only its state; otherwise it sends everything. Firmware before
  v0.10.1 ignores the CRC and always sends everything.

  | Measured (v0.10.1), from the start of the request | |
  |---|---|
  | Request confirmed | 0.75–0.9 s |
  | State (battery, device) arrives | 1.2 s |
  | Done, nothing changed | **1.3 s** |
  | Done, full sync | 3.0 s |

  v0.9 took about 4.7 s: it waited 1.5 s, sent one value a round with the state last, and 4 reports per value.
  `keycandle --bt-listen 6 cached` asks the way the app does and prints the timings.
* **When the Power page opens:** a request for the state only (`0xF0`, id 1), at most every 5 s. The keyboard
  checks the battery first.
* **If a request goes unanswered** (the keyboard was asleep or out of range): the app asks again every few seconds,
  without showing it, until the keyboard answers.
* **A setting changed in the app:** one message on the setting's own channel and id. The keyboard applies and
  confirms it, and doesn't echo it back.
* **A change made on the keyboard** (Fn keys, device switch, battery level): reported on its own, whether or not the
  app is open. The keyboard compares its values every 100 ms, but sends only what changed.

### Cost

Reports share the module's 16-place queue with typing. The keyboard sends at most 2 values (6 packets) per 100 ms
and leaves the rest for keystrokes, so typing during a sync waits a few milliseconds at most. In normal use it
sends a value when an Fn key changes a setting and when the battery level changes. The battery check every minute
stays between the chip and the module, and reaches the radio only when the level changed.

## macOS: Input Monitoring

macOS opens the Bluetooth keyboard to apps only with **Input Monitoring** (System Settings > Privacy &
Security). The app asks on first use (`IOHIDRequestAccess`) and the sidebar says where the switch is until it's on.

macOS remembers the switch for an app's code signature. Up to v0.9b the release bundle was never signed: the
binary only carried the linker's own ad-hoc signature, named `keycandle` and without the bundle's Info.plist.
macOS then didn't match the running app to its switch, so sending failed with the switch on (found 2026-11-19).
Since v0.9.1 the release workflow signs the whole bundle (`codesign --force --deep --sign -`), so it is
identified as `io.github.josemanuque.keycandle`.

If the hint shows with the switch on (e.g. after updating from v0.9b):

1. Input Monitoring: select Keycandle and remove it with **−** (switching it off and on isn't enough).
2. Quit and open Keycandle, allow it when asked, then quit and open it once more.

Or from Terminal: `tccutil reset ListenEvent io.github.josemanuque.keycandle`, then step 2.

An ad-hoc signature changes with every build, so macOS may ask again after an update. Signing with a stable
certificate (self-signed, or an Apple Developer ID) would keep the permission across updates.

## Still unknown

- How the module is powered or reset (A6/A7 are held low as on stock; their role isn't traced).
- Whether P0.15 (configured as an input) is involved.
- Whether the module forwards more than the 3 lock bits of `B1 xx` (Compose, Kana).
