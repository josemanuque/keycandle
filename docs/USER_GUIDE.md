# Keycandle user guide

Keycandle is open firmware for the Royal Kludge RK71 RGB, built on [QMK](https://qmk.fm), with a desktop app to
set it up. This guide covers both, as of **v1.0**. What changed in each version is in the
[changelog](../CHANGELOG.md).

* [Is my keyboard supported?](#is-my-keyboard-supported)
* [Installing](#installing)
* [The app](#the-app)
* [Keys](#keys)
* [Macros](#macros)
* [Wired and Bluetooth](#wired-and-bluetooth)
* [Lighting](#lighting)
* [Side lights and status](#side-lights-and-status)
* [Configuring with VIA](#configuring-with-via)
* [What is saved](#what-is-saved)
* [Limitations](#limitations)
* [Troubleshooting](#troubleshooting)

## Is my keyboard supported?

Keycandle is for the **RK71 RGB with Bluetooth** (tri-mode): 71 keys with per-key RGB, two strips of side lights,
and a battery slide switch on the back. Inside it has the PCB `603-836/RGB-BT-HFD REV:01` and a SONiX
SN32F248BF chip.

The easy check is the app. Plug the keyboard in with the cable and open Keycandle. If it says it found the RK71
on its original firmware, it's this board (the original firmware identifies itself as USB `05AC:024F`). Other RK71
versions (white backlight, wired-only, newer revisions with a different chip) aren't supported. Installing on
one of those won't break the chip, since the bootloader is built into it, but the keys and lights won't match.

## Installing

**With the app** (recommended): plug in the cable, open **Firmware** and click **Install** on the newest
version. It works from the original firmware too.
1. The keyboard restarts into its bootloader by itself.
2. The app checks the file against the release's SHA-256 list and writes it.
3. Once the keyboard is back, the app puts your settings and macros back.

From a terminal: `keycandle --install rk71_rgb_<version>.bin` (settings only, not macros).

**By hand**, from Windows (Git Bash), macOS or Linux:

1. Download `rk71_rgb_<version>.bin` from the [Releases](../../../releases) page, and
   [SonixFlasher](https://github.com/SonixQMK/SonixFlasherC/releases) for your OS into `sonixflasher/` (or your
   PATH). On Linux, also install the udev rule: `sudo cp tools/udev/50-rk71.rules /etc/udev/rules.d/`, then
   `sudo udevadm control --reload && sudo udevadm trigger`.
2. Start `bash tools/flash_rk71.sh rk71_rgb_<version>.bin`. It waits for the bootloader.
3. Put the keyboard into the bootloader (USB `0C45:7040`), with the cable plugged in:
   * **from Keycandle:** **Fn + Pause**
   * **from the original firmware:** `python3 tools/stock_enter_isp.py` (Linux/macOS: `pip install hidapi` first)
   * **if nothing works:** the BOOT-pad short in [stock/README.md](../stock/README.md#if-it-is-bricked-no-keys-no-lights-fnpause-does-nothing)
4. It flashes, verifies the checksum, and the keyboard restarts.

Installing works with the slide switch in either position, but the cable must be plugged in. Firmware never
installs over Bluetooth: the chip's bootloader is USB-only.

**Going back to the original firmware:** in the app, **Firmware ▸ Other firmware ▸ Restore original firmware**. By hand:
[stock/README.md](../stock/README.md#going-back-to-stock).

## The app

| Page | What's there |
|---|---|
| **Overview** | The keyboard as it is now (live preview), what's set, and whether updates are out |
| **Lighting** | Key effect (a gallery of live previews), colour, brightness, speed; side lights |
| **Keys & indicators** | Caps Lock key and side light, F-row lock, Fn highlight, Win-key lock, Windows or Mac per device, and every Fn shortcut (**Show all shortcuts**) |
| **Macros** | Write, record, try and assign macros |
| **Power** | Battery and charging, battery bar, lighting-off and sleep times, dimming on battery |
| **Firmware** | Install updates, a file, or the original firmware; recovery help |
| **Settings** | Language (English, Español, Português), appearance, update checks, slider values as percent or the keyboard's own numbers |

The sidebar's status line shows the connection and what's happening: sending, receiving, saved.
Changes apply to the keyboard at once and are saved on it a moment after you stop adjusting.

**Updates:** when a new app or firmware version is out, a notice appears (a toast, a dot in the sidebar, and a
card on Settings). Nothing installs without you. Settings can turn the check off.

### Over Bluetooth

With the cable unplugged, the app keeps working over Bluetooth:
* you can change your settings;
* changes made on the keyboard with the Fn keys show up in the app;
* the Power page shows the battery and which device is in use.

Macros and firmware updates need the cable. See [how it works](BLUETOOTH.md#settings-over-bluetooth).

* **Windows:** works as is.
* **macOS:** allow Keycandle under **System Settings ▸ Privacy & Security ▸ Input Monitoring** when it asks. If
  the app still asks with the switch on, remove Keycandle from the list with **−** and open it again
  ([why](BLUETOOTH.md#macos-input-monitoring)).
* **Linux:** install the udev rule from the release's README.txt.

When the app finds the keyboard over Bluetooth, the status line shows *Receiving* for about a second while the
keyboard sends its battery, its device and any settings that changed since the cable. Settings it doesn't report back (side lights, power) show their last
values from the cable.

## Keys

The base layer is a normal ANSI 70% layout. Esc is *Grave Escape*: on its own it's Esc, and with Shift it gives
`~`.

### Fn shortcuts

| Fn + | Does |
|---|---|
| Q W E R T | Bluetooth device 1–5. Hold 3 s: pair a new device in that slot. |
| P | Switch wired ↔ Bluetooth. Hold 3 s: pair the current slot again. |
| Backspace | Hold 3 s: forget all paired Bluetooth devices |
| Space | Show the battery level on the side lights |
| A / S | Windows layout / Mac layout (remembered for each Bluetooth device and for the cable) |
| Win | Win-key lock on/off (stops the Win key, for example while gaming) |
| Ctrl (either) | **F-row lock** on/off (remembered). The Fn key glows orange while it's on. |
| Enter | Lighting on/off |
| \\ | Next lighting effect |
| H | Lighting hue (colour) |
| ↑ / ↓ | Lighting brighter / dimmer |
| ← / → | Effect slower / faster |
| 1 … = | F1 … F12 |
| Esc | \` |
| Ins | Print Screen |
| Home / End | Previous / next track |
| Del | Play/pause |
| PgUp / PgDn | Volume up / down |
| M | Mute |
| Pause | Bootloader (for installing firmware) |

Your macros add their own Fn + key. The app lists everything under **Keys & indicators ▸ Show all shortcuts** (also
in the Help menu).

While Fn is held, the P key shows the current mode (green = wired, blue = Bluetooth). In Bluetooth mode, the
current device key (Q–T) is lit blue too.

### F-row lock

With the lock on (Fn + Ctrl), the number row types F1…F12 without Fn, and Fn turns them into media keys:

| Fn + | F1 | F2 | F4 | F5 | F6 | F7 | F8 | F9 |
|---|---|---|---|---|---|---|---|---|
| does | screen dimmer | screen brighter | previous track | play/pause | next track | mute | volume − | volume + |

Prefer media first? On **Keys & indicators**, set *With the F-row lock on, the number row gives* to **Media**. The number row
then gives those media keys on its own, and Fn + the row gives F1…F12.

### Mac layout

Fn + S switches to the Mac layout: the key next to Space becomes Cmd and the next one out becomes Option, on both
sides. Fn + A goes back to Windows. The choice is remembered separately for each Bluetooth device and for the
cable, and the app has a card per device. The Fn combinations use the physical keys, so they're the same in both
layouts.

## Macros

A macro types something when you press **Fn + its key**:
* text, any length, including new lines;
* single keys: Enter, Tab, arrows, F-keys…;
* shortcuts such as Ctrl + Shift + T;
* pauses, for apps that need a moment.

Macros are stored on the keyboard, so they work on any computer, over the cable or Bluetooth.

* **Make one:** **Macros ▸ New macro**, then add text and keys, or press **Record keys** and type: letters become
  text, and other keys and shortcuts become steps.
* **Give it a key:** **Choose key…** and click a key on the keyboard picture. Keys that already do something with
  Fn (media, lighting, devices) aren't offered, so a macro never replaces a shortcut.
* **Try it:** click the test box and press Fn + the key, or click **Play**.
* **Save to keyboard** stores everything. Up to 16 macros share 2 KB, and the page counts the bytes.
* **Ctrl or Cmd, to match the device:** on devices set to Mac, a macro's Ctrl plays as Cmd (and Cmd as Ctrl), so
  Ctrl + C copies on both. Turn it off if your macros are written for one system.

Names stay on the computer; the keyboard stores only the steps. Editing needs the cable and firmware v1.0 or
newer. Macros made in VIA show up in the app, and the other way round.

## Wired and Bluetooth

* **Unplugged:** always Bluetooth (slide switch on).
* **Plugged in:** wired, unless you picked Bluetooth with Fn + P or Fn + Q…T. That choice is remembered. The app
  and VIA work over the cable in either mode. Plugging and unplugging switches automatically: the firmware
  senses the cable.
* **Pair a new computer:**
  1. Hold Fn + Q (or W/E/R/T) for 3 s. The side lights blink blue quickly.
  2. On the computer, pick the keyboard in its Bluetooth settings. The name comes from the Bluetooth module, the
     same as with the original firmware. The side lights go solid blue for a moment once connected.
* **Switch computers:** tap Fn + the slot key.
* **Forget everything:** hold Fn + Backspace 3 s. The side lights flash red.

The slide switch only connects the battery. With it off, the keyboard works over the cable only.

**Battery level:** the Bluetooth module reports only high, medium or low, plus "low" and "shutting down" warnings.
There's no percentage: the module doesn't send one and the chip has no wire to the battery. The keyboard asks the
module every minute, and every few seconds after starting or waking until it answers. The app's Power page asks
again when you open it.

**Charging:** with the cable in, the Power page shows *Charging*, or *Charged* once the module reports the battery
as high. The red light near the space bar is the charger's own light. It's wired to the charging chip, so no
firmware controls it.

## Lighting

Twenty effects: solid colour, breathing, band, gradients, cycles (all, left-right, up-down, pinwheel, spiral),
chevron, rain effects, typing heatmap, reactive, and splashes. Change the effect with Fn + \\ or in the app,
where you also get colour, brightness and speed.

* **Caps Lock key:** pick in **Keys & indicators**.

  | Option | What the key does while Caps Lock is on |
  |---|---|
  | Auto | Turns cool white, as on the original firmware. On white or pale lighting it turns dark instead. |
  | Custom colour | Your colour (white by default) |
  | Dark | Its light goes off |
  | Off | Nothing changes |

  Caps Lock can also show on the nearest side light.
* **Fn highlight:** while Fn is held, only keys that do something with Fn stay lit.
* **Computer asleep:** all lighting goes off.
* **No visible flicker:** the keys are lit one colour row at a time (5 key rows × red, green, blue), so each LED
  is switched fully on and off very fast. Keycandle does it about 1,400 times a second (QMK's driver managed 165).
  That's above the 1,250 Hz that IEEE 1789 rates as low-risk for full on/off flicker, so colours don't split
  apart when your eyes move. The side lights cycle at about 3,100 Hz, above the 3,000 Hz that IEEE 1789 rates as
  having no observable effect. Both are measured on the board: `keycandle --probe` (keys) and `--get 0 53 6` (side
  lights).

### Battery saving

On Bluetooth without the cable, the lighting turns off after 5 minutes without typing and comes back with the next
key press. The keyboard **deep-sleeps** once nothing is lit and you haven't typed for 10 seconds. Asleep, the chip
draws almost nothing, like the original firmware's sleep.

* **Any key wakes it instantly, and that key is typed.** You don't need a "wake-up" key press.
* Plugging in the cable wakes it too.
* It never sleeps while plugged in, while a key is held, or while a status (pairing, mode change, battery bar)
  is showing.
* The lighting also turns off when the battery reports critically low.
* The low-battery blink only shows while you're typing, so it can't keep a nearly flat keyboard awake.

Set both times on the **Power** page:
* lighting off after 1, 2, 5 (default), 10 or 30 minutes, or never;
* deep sleep 5 s, 10 s (default), 30 s, 1 min or 5 min after that, or never.

Also there:
* **Battery bar on the side lights:** shows the battery level when you unplug, and when it gets low.
* **Dim the lighting on battery:** keys and side lights at a lower level without the cable. Full brightness returns
  when you plug in.

## Side lights and status

The two edge strips have 6 lights each (light 1 = far end, 6 = nearest you). Both strips always show the same
thing.

**Normally** they follow the keyboard's colour, at the keyboard's brightness, and go off when the lighting is off.
You can pick a fixed colour or turn them off, and choose an effect with its own speed:
* static;
* breathing;
* colour cycle;
* type pulse (brightens as you type);
* wave.

**Same brightness as the keys** (on by default) uses the keys' brightness; turn it off to give them their own.

**How they work:** each light has separate red, green and blue chips, and the left and right light at the same
position share one switch. To show a colour, the firmware lights red, then green, then blue, each for a share of a
frame about 0.32 ms long, about 3,100 times a second; the eye mixes them. The on-times set the colour and the
brightness.

### Side-light colour limitations

The side lights can't show colours as exactly as the keys:

* **Mixed colours are approximate**, mostly at low brightness. There a colour's share can be about a microsecond,
  and LEDs don't light in exact proportion to such short pulses, so a colour can lean a little toward one hue.
  Pure red, green or blue look the most accurate.
* **Lights differ from each other:** some look brighter or whiter than others. Every LED's chips are slightly
  different, and the diffuser in front of them is bluish. The per-light trims (VIA, *Calibration*) even this out a
  little, but a left and right light share a switch, so they can't be trimmed separately.
* **Colour balance is personal:** it depends on the board and your eyes. The Red / Green / Blue sliders balance the
  bluish diffuser. The defaults are 31 / 15 / 23; on the author's board, 23 / 5 / 8 looks the most natural.

**Indicators:** Caps Lock on the nearest light and F-row lock on the farthest one, each in a colour you pick. They
show while the lighting is on or you're typing.

**Status** takes over for a moment when something happens (unless turned off):

| What you see | Meaning |
|---|---|
| Fast blue blink | Pairing (waiting for a computer) |
| Slow blue blink | Connecting to the selected device |
| Solid blue, ~1 s | Connected |
| Solid green, 1.5 s | Switched to wired |
| Solid blue, 1.5 s | Switched to Bluetooth |
| Fast red flash, all | All pairings forgotten |
| Fn + Space: 6 green | Battery high |
| Fn + Space: 3 yellow (near end) | Battery medium |
| Fn + Space: 1 red (nearest) | Battery low |
| Fn + Space: nearest blinks red | No answer from the Bluetooth module (slide switch off?) |
| Nearest light, slow red blink | Battery low, charge soon |

The wired and Bluetooth status colours and the status brightness can be changed in VIA.

The connect and pair indication only shows after you press a Bluetooth key. Automatic reconnects (after waking up,
for example) are silent.

With side-light status turned off, the same information shows on the keys instead:
* the slot key (Q–T) blinks blue;
* P flashes green or blue;
* Backspace flashes red;
* Space blinks red for low battery.

The Fn + Space battery bar exists only on the side lights, so with status off it shows nothing.

## Configuring with VIA

[VIA](https://usevia.app) runs in Chrome or Edge, with nothing to install, on any system. Keycandle firmware works
with it over the cable.

1. Open [usevia.app](https://usevia.app).
2. The first time only:
   1. In **Settings**, turn on "Show Design tab".
   2. Under **Design**, load `rk71_rgb_via.json` from the [latest release](../../../releases/latest).
3. **Authorize device**, then pick the RK71.

You get:

* **Configure ▸ Keymap:** remap any key on all 5 layers:

  | Layer | |
  |---|---|
  | 0 | Windows |
  | 1 | Mac |
  | 2 | F-row lock |
  | 3 | Fn |
  | 4 | Fn + F-row media |

  The Bluetooth keys are under *Special ▸ Custom*: BT 1–5, BT/USB, BT Clear, Battery.
* **Macros:** the same macros as the app's.
* **Lighting:** effect, brightness, speed, colour.
* **Side lights:** colour, effect and speed, brightness, Red / Green / Blue balance, status and its colours, and
  **Calibration** (per-light trims while all lights are held plain white, and a low-brightness correction per
  colour).
* **Indicators:** Caps Lock and F-row lock on the side lights, the F-row order, the Caps Lock key, Fn highlight,
  Win-key lock.
* **Windows or Mac:** Mac per Bluetooth device and for the cable, and Ctrl as Cmd in macros.
* **Power:** lighting-off and sleep times, battery bar, dimming on battery.

VIA has to use the cable. Settings are saved about 2 seconds after you stop moving a slider.

## What is saved

Kept across unplugging and power-off:

* keymap, macros, lighting, side-light, indicator and power settings;
* Windows or Mac per device, Win-key lock, F-row lock, wired or Bluetooth, the current device slot.

Bluetooth pairings live in the Bluetooth module itself, so they survive installing firmware.

**Installing firmware resets everything else to defaults** (QMK starts fresh with each new build). The app saves
your settings and macros first and puts them back afterwards. Keymap changes made in VIA aren't restored.

## Limitations

* **No battery percentage:** the module only reports high, medium or low.
* **While asleep, the keyboard doesn't hear the computer.** A Caps Lock change made on the computer while the
  keyboard is asleep shows on the Caps key only after the next change. The Bluetooth module itself stays connected.
* **Over Bluetooth**, side-light and power settings don't come back from the keyboard. The app shows them as they
  were last seen over the cable. Macros and firmware need the cable.
* **Storage is tight:** the chip has 8 KB of RAM, so 5 layers is the maximum.
* **Bootmagic (hold Esc while plugging in)** only works if the chip really restarts, so slide the battery switch
  off first. Fn + Pause is easier.

## Troubleshooting

| Problem | Try |
|---|---|
| Keys don't work and the lights are off after installing | Recovery via the BOOT pad: [stock/README.md](../stock/README.md#if-it-is-bricked-no-keys-no-lights-fnpause-does-nothing) |
| After Fn + Pause the keyboard seems dead | It's in the bootloader, waiting for firmware. Install from the app (it offers **Leave flash mode** too), or unplug, slide the switch off and plug in again. |
| The app doesn't find the keyboard | Use a data cable (some only charge). On Linux, install the udev rule. |
| Over Bluetooth, the app says settings go one way | The keyboard's firmware is older than v0.9: install the latest over the cable. |
| macOS: Bluetooth settings ask for Input Monitoring with it on | Remove Keycandle from Input Monitoring with **−**, then open it again ([details](BLUETOOTH.md#macos-input-monitoring)) |
| A Bluetooth computer won't connect | Remove the keyboard from the computer's Bluetooth list, then hold Fn + slot key 3 s to pair again |
| Fn + Space blinks red | The Bluetooth module isn't answering: slide switch off, or the battery is completely flat |
| Want the original firmware back | App: **Firmware ▸ Restore original firmware**, or [stock/README.md](../stock/README.md#going-back-to-stock) |

For building, the project's layout and hardware details, see [DEVELOPMENT.md](DEVELOPMENT.md).
