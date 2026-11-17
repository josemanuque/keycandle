# RK71 QMK firmware: user guide

An open-source replacement for the Royal Kludge RK71 RGB's factory firmware, built on
[QMK](https://qmk.fm) (SonixQMK fork). You configure it live from the browser with [VIA](https://usevia.app),
on any OS, with no vendor software.

This guide describes the latest version (`main`, currently **v0.8.1b**). The change history is in the
[README](../README.md#branches-and-versions).

* [Stock vs this firmware](#stock-vs-this-firmware)
* [Installing](#installing)
* [Keys](#keys)
* [Wired and Bluetooth](#wired-and-bluetooth)
* [Lighting](#lighting)
* [Side lights and status](#side-lights-and-status)
* [Configuring with VIA](#configuring-with-via)
* [What is saved](#what-is-saved)
* [Limitations](#limitations)
* [Troubleshooting](#troubleshooting)
* [For developers](#for-developers)

## Stock vs this firmware

| | Stock V1.12 | This firmware |
|---|---|---|
| Source code | Closed binary (vendor Sonix/HFD SDK) | Open (QMK, GPL-2.0), in `keyboard/rk71_rgb` |
| Configuration | Royal Kludge's Windows app only | VIA in any Chromium browser (Windows, macOS, Linux), live, no install |
| Remapping | What the RK app allows | Every key on every layer, 5 layers, macros, any QMK keycode |
| Lighting | Built-in effects, custom per-key colours in the RK app | 19 QMK effects + solid colour, hue/saturation/speed in VIA |
| Light refresh | Not measured | Keys ~1.4 kHz, side lights ~3.1 kHz, measured; inside IEEE 1789's flicker guidance ([details](#lighting)) |
| Bluetooth | 5 devices (Fn+Q…T) | Same keys, same 5 devices, plus automatic wired/Bluetooth from the cable |
| Side lights | Status only | Follow keyboard colour / fixed colour / off, 5 effects, own brightness, Caps Lock and F-row lock indicators, status overlay, per-light calibration |
| Battery level | – | Fn+Space shows a 1/3/6-light bar on the side lights |
| Debounce | Vendor default | 10 ms per key (fixes chattering switches without slowing the others) |
| USB ID | `05AC:024F` (pretends to be an Apple keyboard) | `0C45:8071` (its own ID) |
| Sleep on battery | Deep sleep | Deep sleep: instant wake on any key, and that key is typed; all times set in VIA ([details](#battery-saving)) |
| Updates | RK's Windows updater | The RK71 Configurator app (`app/`), or the flash script below |

The RK configuration app doesn't work with this firmware, and VIA doesn't work with the stock firmware. To go
back to stock at any time, see [stock/README.md](../stock/README.md).

## Installing

**Already on v0.7b or later:** the RK71 Configurator app (`app/`) installs releases from its Firmware page: the
keyboard restarts into the bootloader by itself, the image is checked against the release's SHA-256 list, and
your settings are put back afterwards. It can also switch from the stock firmware. From a terminal:
`rk71-configurator --install rk71_rgb_<version>.bin`.

**By hand**, from Windows (Git Bash), macOS and Linux:

1. Download `rk71_rgb_<version>.bin` from the [Releases](../../../releases) page, and
   [SonixFlasher](https://github.com/SonixQMK/SonixFlasherC/releases) for your OS into `sonixflasher/` (or your PATH).
   On Linux, also install the udev rule: `sudo cp tools/udev/50-rk71.rules /etc/udev/rules.d/`, then
   `sudo udevadm control --reload && sudo udevadm trigger`.
2. Put the keyboard into the bootloader (USB `0C45:7040`). With the cable plugged in:
   * **from this firmware:** **Fn + Pause**
   * **from stock firmware:** `python3 tools/stock_enter_isp.py` (Linux/macOS: `pip install hidapi` first)
   * **if nothing works:** the BOOT-pad short in [stock/README.md](../stock/README.md#if-it-is-bricked-no-keys-no-lights-fnpause-does-nothing)
3. `bash tools/flash_rk71.sh rk71_rgb_v0.7b.bin`

   Tip: start the script first. It waits for the bootloader, so you can then press Fn+Pause and it flashes
   straight away.
4. The keyboard restarts on its own. The first boot after a full flash starts from defaults: solid white
   lighting, wired, device slot 1.

Flashing works with the slide switch in either position, but the cable must be plugged in.

## Keys

The base layer is a normal ANSI 70% layout (see `keymaps/via/keymap.c`). Esc is *Grave Escape*: on its own it's
Esc, and with Shift it gives `~`.

### Fn layer

| Fn + | Does |
|---|---|
| Esc | \` |
| 1 … = | F1 … F12 |
| Ins / Home | Print Screen / previous track |
| Del / End | Play-pause / next track |
| PgUp / PgDn | Volume up / down |
| M | Mute |
| Enter | Lighting on / off |
| \\ | Next lighting effect |
| H | Lighting hue (colour) |
| ↑ / ↓ | Lighting brighter / dimmer |
| ← / → | Effect slower / faster |
| A / S | Windows mode / Mac mode (remembered for each Bluetooth device and for the cable) |
| Win | Win key lock on/off (stops the Win key, e.g. while gaming) |
| Ctrl (either) | **F-row lock** on/off (remembered). The Fn key glows orange while it's on. |
| Q W E R T | Bluetooth device 1–5. Hold 3 s: pair a new device in that slot. |
| P | Switch wired ↔ Bluetooth. Hold 3 s: pair the current slot again. |
| Backspace | Hold 3 s: forget all paired Bluetooth devices |
| Space | Show the battery level on the side lights |
| Pause | Bootloader (for flashing) |

While Fn is held, the P key shows the current mode (green = wired, blue = Bluetooth). In Bluetooth mode, the
current device key (Q–T) is lit blue too.

### F-row lock

With the lock on (Fn+Ctrl), the number row types F1…F12 without Fn. Fn then turns the F-keys into media keys:

| Fn + | F1 | F2 | F4 | F5 | F6 | F7 | F8 | F9 |
|---|---|---|---|---|---|---|---|---|
| does | screen dimmer | screen brighter | prev track | play-pause | next track | mute | vol − | vol + |

### Mac mode

Fn+S switches to the Mac layer: the key next to Space becomes Cmd and the next one out becomes Option, on both
sides. Fn+A goes back to Windows. The choice is remembered separately for each Bluetooth device and for the cable,
so a Mac and a PC can share the keyboard. The Fn combinations use the physical keys, so they're the same in both
modes.

## Wired and Bluetooth

* **Unplugged:** always Bluetooth (slide switch on).
* **Plugged in:** wired, unless you picked Bluetooth with Fn+P or Fn+Q…T. That choice is remembered. In Bluetooth
  mode with the cable plugged in, VIA and the configurator still work over the cable (since v0.8).
  Unplugging and plugging in switches automatically: the firmware senses the cable.
* **Pair a new computer:**
  1. Hold Fn + Q (or W/E/R/T) for 3 s. The side lights blink blue quickly.
  2. On the computer, pick the keyboard in its Bluetooth settings (the name comes from the Bluetooth module, the
     same as with stock firmware). The side lights go solid blue for a moment once connected.
* **Switch computers:** tap Fn + the slot key.
* **Forget everything:** hold Fn+Backspace 3 s. The side lights flash red.

The slide switch only connects the battery. With it off, the keyboard works over the cable only.

**Battery level:** the Bluetooth module reports only high, medium or low (and "low" / "shutting down" warnings),
shown with Fn+Space and on the configurator's Power page. There is no percentage: the module doesn't send one
and the chip has no wire to the battery. Windows can't show one either (the keyboard uses classic Bluetooth).

**Settings over Bluetooth** aren't possible yet: the module offers only keyboard and media interfaces, with no
data channel like VIA's. Use the cable for VIA and the configurator; the keyboard keeps every setting when you
unplug. A one-way way to send settings over Bluetooth is planned for v0.9 (see the README's roadmap).

## Lighting

19 QMK effects plus solid colour: breathing, band, cycles (all, left-right, up-down, pinwheel, spiral),
gradients, chevron, rain effects, reactive (lights up on key press), splash, typing heatmap. Change the effect
with Fn+\\ or in VIA, where you also get hue, saturation, brightness and speed.

* **Caps Lock:** the Caps key turns cool white (as on stock). On white or pale lighting, where that wouldn't
  stand out, it turns dark instead. In VIA (*Indicators*) you can turn this off or pick your own colour, and
  show Caps Lock on the nearest side light too.
* **Fn highlight** (VIA, *Indicators*): while Fn is held, only keys that do something with Fn stay lit.
* **Computer asleep:** all lighting goes off.
* **On battery:** lighting turns off after 5 minutes without typing (adjustable in VIA, *Power*), and comes back
  with the next key press. It also turns off when the battery reports critically low. VIA can also dim it on
  battery.
* **No visible flicker:** the keys are lit one colour row at a time (5 key rows x red/green/blue), so each LED is
  switched fully on and off very fast. This firmware does it ~1400 times a second (QMK's driver managed 165), above
  the 1250 Hz that IEEE 1789 rates as low-risk for full on/off flicker, so colours don't split apart when your eyes
  move. The side lights cycle at ~3100 Hz, above the 3000 Hz that IEEE 1789 rates as having no observable effect.
  Both are measured on the board: `rk71-configurator --probe` (keys) and `--get 0 53 6` (side lights).

### Battery saving

On Bluetooth without the cable, the keyboard **deep-sleeps** once nothing is lit and you haven't typed for
10 seconds. That happens 10 seconds after you stop typing if your lighting is off, or right after the 5-minute
lights-off otherwise. Asleep, the chip draws almost nothing, like the stock firmware's sleep.

* **Any key wakes it instantly, and that key is typed.** You don't need a "wake-up" key press.
* Plugging in the cable wakes it too.
* It never sleeps while plugged in, while a key is held, or while a status (pairing, mode change, battery bar)
  is showing.
* The low-battery blink only shows while you're typing, so it can't keep a nearly flat keyboard awake.

Set both times in VIA under *Power*: lighting off after 1, 2, 5 (default), 10 or 30 minutes or never; deep sleep
5 s, 10 s (default), 30 s, 1 min or 5 min after that, or never. Also there:

* **Battery bar when unplugging or low:** the side lights show the battery level when you unplug, and when it
  gets low.
* **Dim the lighting on battery:** keys and side lights at a lower level (*Brightness on battery*) without the
  cable; full brightness returns when you plug in.

## Side lights and status

The two edge strips have 6 lights each (light 1 = far end, 6 = nearest you). Both strips always show the same
thing.

**Normally** they follow the keyboard's colour, at the keyboard's brightness, and go off when the lighting is off.
In VIA or the app you can pick a fixed colour or turn them off, and choose an effect: static, breathing, colour
cycle, type pulse (brightens as you type) or wave, with its own speed. **Same brightness as the keys** (on by
default) uses the keys' brightness; turn it off to give them their own (the slider appears then).

**How they work:** each light has separate red, green and blue chips, and the left and right light at the same
position share one switch. To show a colour, the firmware lights red, then green, then blue, each for a share of
a ~0.32 ms frame, about 3100 times a second; the eye mixes them. The on-times set the colour and the brightness.

### Side-light colour limitations

The side lights can't show colours as exactly as the keys:

* **Mixed colours are approximate**, most at low brightness. There a colour's share can be about a microsecond,
  and LEDs don't light in exact proportion to such short pulses, so a colour can lean a little toward one hue.
  Pure red, green or blue look the most accurate.
* **Lights differ from each other**: some look brighter or whiter than others. Every LED's chips are slightly
  different, and the diffuser in front of them is bluish. The per-light trims (VIA, *Calibration*) even this out
  a little, but a left and right light share a switch, so they can't be trimmed separately.
* **Colour balance is personal:** it depends on the board and your eyes. On the author's board, **Red 31, Green 3,
  Blue 5** (with the low-brightness correction at 0 / 8 / 5) looked the most natural. The defaults are 31 / 15 / 23.

**Indicators** (VIA, *Indicators*): Caps Lock on the nearest light and F-row lock on the farthest one, each in a
colour you pick. They show while the lighting is on or you're typing.

**Status** takes over for a moment when something happens (unless turned off in VIA):

| What you see | Meaning |
|---|---|
| Fast blue blink | Pairing (waiting for a computer) |
| Slow blue blink | Connecting to the selected device |
| Solid blue, ~1 s | Connected |
| Solid green, 1.5 s | Switched to wired |
| Solid blue, 1.5 s | Switched to Bluetooth |
| Fast red flash, all | All pairings forgotten |
| Fn+Space: 6 green | Battery high |
| Fn+Space: 3 yellow (near end) | Battery medium |
| Fn+Space: 1 red (nearest) | Battery low |
| Fn+Space: nearest blinks red | No answer from the Bluetooth module (slide switch off?) |
| Nearest light slow red blink | Battery low, charge soon |

The wired and Bluetooth status colours and the status brightness can be changed in VIA.

The connect/pair indication only shows after you press a Bluetooth key. Automatic reconnects (after waking up,
for example) are silent.

With side-light status turned off in VIA, the same information shows on the keys instead: the slot key
(Q–T) blinks blue, P flashes green/blue, Backspace red, and Space blinks red for low battery. The Fn+Space
battery bar exists only on the side lights, so with status off it shows nothing.

**The red LED near the space bar** is the charger's own "charging" light. It's wired directly to the charging
chip, so no firmware can control it.

## Configuring with VIA

1. Open [usevia.app](https://usevia.app) in Chrome or Edge.
2. The first time only: in **Settings**, turn on "Show Design tab". Then under **Design**, load
   `via/rk71_rgb_via.json` from this repository. VIA doesn't know this board otherwise.
3. **Authorize device**, then pick the RK71.

You get:

* **Configure → Keymap:** remap any key on all 5 layers (0 Windows, 1 Mac, 2 F-row lock, 3 Fn, 4 Fn+F-row
  media). The Bluetooth functions are under *Special → Custom*: BT 1–5, BT/USB, BT Clear, Battery.
* **Macros:** text and key sequences, assignable to any key.
* **Lighting:** effect, brightness, speed, colour.
* **Side lights:**
  * **Colour:** Follow keyboard / Fixed colour / Off (and which fixed colour)
  * **Effect** and **Effect speed**
  * **Same brightness as the keys** (default on), or **Brightness** 0–31 of their own when it's off
  * **Red / Green / Blue:** 0–31 each. The side lights sit behind a bluish diffuser, and these sliders balance it
    out. Defaults are 31 / 15 / 23, tuned by eye on a real board.
  * **Show status**, **Status brightness**, **Status colour** (wired, Bluetooth)
  * **Calibration:** per-light red/green/blue trims (with all lights held plain white while you adjust), and a
    low-brightness correction per colour. Each LED's chips differ a little, so this evens them out.
* **Indicators:** Caps Lock and F-row lock on the side lights, Caps Lock key colour, Fn highlight.
* **Power:** lighting-off and sleep times, battery bar, dim on battery.

The RK71 Configurator app (`app/`) shows the same settings with live previews of every effect.

VIA has to be connected over USB. Settings are saved about 2 seconds after you stop moving a slider.

Each light has slightly different red, green and blue chips, so a mixed colour like white can look a little
different from light to light. That's normal for these LEDs. Pure red, green or blue look the most even.

## What is saved

Kept across unplugging and power-off:

* Keymap, macros, lighting effect and colour, side-light, indicator and power settings
* Win/Mac mode (per device), Win key lock, F-row lock, wired/Bluetooth choice, current device slot

Bluetooth pairings are stored in the Bluetooth module itself, so they survive reflashing. Everything else resets
to defaults when you flash a new image or switch to stock and back. The Configurator app saves its settings before
an update and puts them back afterwards (not the keymap).

## Limitations

* **While asleep, the keyboard doesn't hear the computer.** A Caps Lock change made on the computer while the
  keyboard is asleep shows up on the Caps key only after the next change. The Bluetooth module itself stays
  connected.
* **Storage is tight:** 5 layers is the maximum alongside VIA macros. The chip has 8 KB of RAM.
* **Bootmagic (hold Esc while plugging in)** only works if the chip actually restarts, so slide the battery
  switch off first. Fn+Pause is easier.
* **Status lights:** Caps Lock may blink once when switching to Bluetooth.

## Troubleshooting

| Problem | Try |
|---|---|
| Keys don't work, lights off after flashing | Recovery via the BOOT pad: [stock/README.md](../stock/README.md#if-it-is-bricked-no-keys-no-lights-fnpause-does-nothing) |
| Fn+Pause and the keyboard seems dead | It's in the bootloader, waiting for a flash. Install from the app or the flash script; or click *Leave flash mode* in the app; or unplug, slide switch off, plug in again. |
| VIA doesn't find the keyboard | Use the cable, use Chrome/Edge, and load `via/rk71_rgb_via.json` in the Design tab. On Linux, install `tools/udev/50-rk71.rules`. |
| Bluetooth computer won't connect | Remove the keyboard from the computer's Bluetooth list, then hold Fn + slot key 3 s to pair again |
| Fn+Space blinks red | The Bluetooth module isn't answering: slide switch off, or the battery is completely flat |
| Want factory firmware back | [stock/README.md](../stock/README.md#going-back-to-stock) |

## For developers

* Source: `keyboard/rk71_rgb/`
  * `rk71_rgb.c`: modes, Bluetooth keys, status, VIA menu
  * `rk71_bt.c`: SPI driver for the Bluetooth module
  * `rk71_side.c`: side lights and cable detection
  * `rk71_sleep.c`: deep sleep and wake-up; `rk71_keyled.c`: dimming on battery
  * `sn32f2xx.c`: the key LED driver (replaces SonixQMK's: a short row switch, so the keys refresh at ~1430 Hz)
  * `keymaps/via`: the keymap
* VIA values 50 and 51 (read-only) report the key LEDs' measured refresh and row-switch time;
  `rk71-configurator --probe` prints them.
* `keyboard.json` and the VIA definition are generated by `tools/gen_rk71.py`. Edit that, not the outputs.
* Build: `bash tools/build.sh` (setup in the [README](../README.md#building)).
* Hardware findings: [BLUETOOTH.md](BLUETOOTH.md) (module protocol), [stock/FIRMWARE_NOTES.md](../stock/FIRMWARE_NOTES.md)
  (stock key map, ISP entry). The `diag` keymap toggles and shows unknown pins, which helps when mapping
  hardware.
