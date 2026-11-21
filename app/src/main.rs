//! Keycandle: settings and firmware updates for the RK71 QMK firmware.
//!
//!   keycandle            open the app
//!   keycandle --page firmware    open on a page (overview, lighting, keys, power, firmware)
//!   keycandle --probe    print every keyboard setting in the terminal (USB test)
//!   keycandle --releases list the firmware releases on GitHub (download test)
//!   keycandle --isp-info read the bootloader's chip id and flash checksum (read-only)
//!   keycandle --leave-flash-mode   start the firmware from the bootloader
//!   keycandle --install <file.bin>  install firmware, like the Firmware page
//!   keycandle --set <channel> <id> <byte>…   write and save one VIA value
//!   keycandle --get <channel> <id> <length>   read one VIA value
//!   keycandle --bt-set <channel> <id> <byte>…  send one VIA value over Bluetooth (Windows)

// A windowed app on Windows (no console window when double-clicked); see attach_console() for the
// command-line options.
#![cfg_attr(windows, windows_subsystem = "windows")]

mod btlink;
mod color;
mod device;
mod i18n;
mod isp;
mod keyboard;
mod macros;

/// Shown in the About panel (macOS), on Settings and in the Windows file properties (build.rs has its own copy).
pub const COPYRIGHT: &str = "© 2026 josemanuque";
mod prefs;
mod preview;
mod releases;
mod settings;
mod ui;
mod updater;

use anyhow::Result;

// Translations in locales/*.yml, compiled in; see i18n.rs.
rust_i18n::i18n!("locales", fallback = "en");
use device::{Keyboard, CHANNEL_KEYBOARD, CHANNEL_RGB_MATRIX};

fn main() -> Result<()> {
    if std::env::args().nth(1).is_some_and(|a| a.starts_with("--") && a != "--page") {
        attach_console();
    }
    i18n::init();
    match std::env::args().nth(1).as_deref() {
        Some("--probe") => probe(),
        Some("--releases") => list_releases(),
        Some("--isp-info") => isp_info(),
        Some("--leave-flash-mode") => isp::Bootloader::open()?.start_firmware(),
        Some("--install") => install(std::env::args().nth(2).as_deref()),
        Some("--set") => set_value(&std::env::args().skip(2).collect::<Vec<_>>()),
        Some("--get") => get_value(&std::env::args().skip(2).collect::<Vec<_>>()),
        Some("--bt-set") => bt_set(&std::env::args().skip(2).collect::<Vec<_>>()),
        Some("--timing") => timing(),
        Some("--macros") => macros_cli(&std::env::args().skip(2).collect::<Vec<_>>()),
        // `--bt-listen [seconds] [cached]`: with "cached", ask as the app does, with its remembered values.
        Some("--bt-listen") => btlink::listen_for(
            std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(6),
            std::env::args().nth(3).filter(|a| a == "cached").and_then(|_| ui::model::Model::cached_bluetooth_values()),
        ),
        Some("--page") => {
            ui::run(std::env::args().nth(2).as_deref());
            Ok(())
        }
        _ => {
            ui::run(None);
            Ok(())
        }
    }
}

/// Command-line USB test: read and print every setting.
fn probe() -> Result<()> {
    let kb = Keyboard::open()?;
    println!("Connected. VIA protocol version {}", kb.protocol_version);
    match kb.firmware_version() {
        Ok(0) => println!("Firmware version: older than 0.7 (not reported)"),
        Ok(v) => println!("Firmware version: {}", releases::format_version(v)),
        Err(e) => println!("Firmware version: ({e})"),
    }
    println!(
        "Key lighting: brightness {:?} effect {:?} speed {:?} color (hue, sat) {:?}",
        kb.get(CHANNEL_RGB_MATRIX, 1, 1)?,
        kb.get(CHANNEL_RGB_MATRIX, 2, 1)?,
        kb.get(CHANNEL_RGB_MATRIX, 3, 1)?,
        kb.get(CHANNEL_RGB_MATRIX, 4, 2)?
    );
    if let Ok(v) = kb.get(CHANNEL_KEYBOARD, 50, 2) {
        let rows = u16::from_be_bytes([v[0], v[1]]) as f32;
        if rows > 0.0 {
            println!("Key LEDs: {rows} rows/s = {:.0} Hz refresh, {:.1} us per row", rows / 15.0, 1e6 / rows);
            if let Ok(s) = kb.get(CHANNEL_KEYBOARD, 51, 6) {
                let ticks = |i: usize| u16::from_be_bytes([s[i], s[i + 1]]) as f32;
                let (gap, max, wait) = (ticks(0), ticks(2), ticks(4));
                // A row is 200 timer ticks lit, then the gap (the timer waits for the switch).
                let tick_us = 1e6 / rows / (200.0 + gap);
                println!(
                    "Gap between rows: {:.1} us ({:.1} us until the switch starts, {:.1} us switching), longest {:.1} us; lit {:.0}% of the time",
                    gap * tick_us,
                    wait * tick_us,
                    (gap - wait) * tick_us,
                    max * tick_us,
                    200.0 / (200.0 + gap) * 100.0
                );
            }
        }
    }
    for id in 1..=49u8 {
        match kb.get(CHANNEL_KEYBOARD, id, 2) {
            Ok(v) => println!("  keyboard value {id:2}: {v:?}"),
            Err(e) => println!("  keyboard value {id:2}: ({e})"),
        }
    }
    Ok(())
}

/// Command-line test: what the Firmware page would offer, with a verified download of the newest.
fn list_releases() -> Result<()> {
    let list = releases::fetch()?;
    for r in &list {
        let version = r.version().map_or("-".into(), releases::format_version);
        println!("{:<14} {:<10} {:<8} {}{}", r.tag, r.date, version, r.file_name(), if r.prerelease { "  (test build)" } else { "" });
    }
    if let Some(newest) = list.iter().find(|r| !r.stock) {
        let image = newest.download()?;
        isp::check_image(&image)?;
        println!("Downloaded {} ({} bytes): SHA-256 matches, looks like valid firmware.", newest.file_name(), image.len());
    }
    Ok(())
}

/// Command-line test of the bootloader link; reads only.
fn isp_info() -> Result<()> {
    let bl = isp::Bootloader::open()?;
    println!("Bootloader: SN32F24xB, code security {:#06x}, flash checksum {:#06x}", bl.security, bl.flash_checksum()?);
    Ok(())
}

/// Command-line install (the Firmware page's "From a file…").
fn install(path: Option<&str>) -> Result<()> {
    let path = path.ok_or_else(|| anyhow::anyhow!("usage: --install <firmware.bin>"))?;
    let source = updater::Source::File(path.into());
    // Like the app: read every setting first, put them back once the new firmware is up.
    let mut model = ui::model::Model::default();
    let saved = model.connect().then(|| model.snapshot());
    let keyboard = model.take_keyboard();
    let last = std::cell::Cell::new(String::new());
    updater::install_blocking(&source, keyboard, &|step| {
        let line = match step {
            updater::Step::Downloading => "Reading the file…".to_string(),
            updater::Step::WaitingForBootloader { manual: false } => "Restarting into flash mode…".into(),
            updater::Step::WaitingForBootloader { manual: true } => "Press Fn + Pause on the keyboard…".into(),
            updater::Step::Writing(p) => format!("Writing {:>3.0}%", (p * 10.0).floor() * 10.0),
            updater::Step::Starting => "Starting the keyboard…".into(),
            _ => return,
        };
        if line != last.replace(line.clone()) {
            println!("{line}");
        }
    })?;
    println!("Installed {}.", source.describe());
    if let Some(saved) = saved {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        while !model.connect() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        if model.is_connected() {
            let wrong = model.restore(&saved);
            if wrong.is_empty() {
                println!("Settings put back (all read back correctly).");
            } else {
                for ((ch, id), want, got) in wrong {
                    println!("Setting {ch}/{id}: wanted {want:?}, keyboard has {got:?}");
                }
            }
        } else {
            println!("The keyboard didn't come back in time; settings were not put back.");
        }
    }
    Ok(())
}

/// Command-line write of one value (`--set 3 4 191 74` = key color hue 191, saturation 74).
fn set_value(args: &[String]) -> Result<()> {
    let numbers: Vec<u8> = args.iter().map(|a| a.parse()).collect::<Result<_, _>>()?;
    let [channel, id, value @ ..] = numbers.as_slice() else {
        anyhow::bail!("usage: --set <channel> <id> <byte>…");
    };
    let kb = Keyboard::open()?;
    kb.set(*channel, *id, value)?;
    kb.save(*channel)?;
    println!("Set {channel}/{id} = {value:?}");
    Ok(())
}

/// Command-line read of one value (`--get 0 2 2` = side-light fixed colour).
fn get_value(args: &[String]) -> Result<()> {
    let numbers: Vec<usize> = args.iter().map(|a| a.parse()).collect::<Result<_, _>>()?;
    let [channel, id, len] = numbers.as_slice() else {
        anyhow::bail!("usage: --get <channel> <id> <length>");
    };
    let kb = Keyboard::open()?;
    println!("{channel}/{id} = {:?}", kb.get(*channel as u8, *id as u8, *len)?);
    Ok(())
}

/// Command-line Bluetooth write (`--bt-set 0 8 20`): sends one value through the lock LEDs.
fn bt_set(args: &[String]) -> Result<()> {
    let numbers: Vec<u8> = args.iter().map(|a| a.parse()).collect::<Result<_, _>>()?;
    let [channel, id, value @ ..] = numbers.as_slice() else {
        anyhow::bail!("usage: --bt-set <channel> <id> <byte> [byte]");
    };
    if value.is_empty() || value.len() > 2 {
        anyhow::bail!("usage: --bt-set <channel> <id> <byte> [byte]");
    }
    let attempts = btlink::send(*channel, *id, value)?;
    println!("Set {channel}/{id} = {value:?} over Bluetooth (confirmed, attempt {attempts})");
    Ok(())
}

/// Windows: a windowed app has no console, so the command-line options print into the terminal
/// they were started from. Elsewhere there's nothing to do.
fn attach_console() {
    #[cfg(windows)]
    {
        #[link(name = "kernel32")]
        extern "system" {
            fn AttachConsole(process: u32) -> i32;
        }
        const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
        // SAFETY: plain Win32 call; failure (no parent console) just means output goes nowhere.
        unsafe {
            AttachConsole(ATTACH_PARENT_PROCESS);
        }
    }
}

/// Diagnostics (--timing): how long each step of finding the keyboard takes, as the app does it.
fn timing() -> Result<()> {
    use std::time::Instant;
    let time = |name: &str, f: &dyn Fn() -> String| {
        let t = Instant::now();
        let result = f();
        println!("{:>6} ms  {name}: {result}", t.elapsed().as_millis());
    };
    time("HID scan", &|| device::hid_api().map(|a| format!("{} devices", a.device_list().count())).unwrap_or_else(|e| e.to_string()));
    time("USB keyboard", &|| Keyboard::open().map(|_| "found".into()).unwrap_or_else(|e| e.to_string()));
    time("flash mode", &|| isp::is_present().to_string());
    time("original firmware", &|| isp::stock_present().to_string());
    time("Bluetooth keyboard", &|| btlink::available().to_string());
    Ok(())
}

/// Diagnostics: `--macros` lists them; `--macros set <n> <text>` (a literal \n for Enter),
/// `--macros key <n> <key label>` puts macro n on Fn + key, `--macros play <n>` types it now.
fn macros_cli(args: &[String]) -> Result<()> {
    let kb = Keyboard::open()?;
    let profile = keyboard::current();
    let (count, size) = kb.macro_space()?;
    let mut all = macros::decode_all(&kb.read_macros(size)?, count);
    match args.first().map(String::as_str) {
        None => {
            println!("{count} macros, {size} bytes ({} used)", macros::size_of(&all));
            for (n, m) in all.iter().enumerate().filter(|(_, m)| !m.is_empty()) {
                println!("  {n}: {m:?}");
            }
            let (rows, cols) = profile.matrix;
            let layer = kb.read_layer(profile.fn_layer, rows, cols)?;
            for key in profile.keys {
                if let Some(n) = macros::macro_of(layer[key.row * cols + key.col]) {
                    println!("  Fn + {} runs macro {n}", key.label);
                }
            }
        }
        Some("set") => {
            let n: usize = args.get(1).and_then(|a| a.parse().ok()).ok_or_else(|| anyhow::anyhow!("which macro?"))?;
            let text = args.get(2).cloned().unwrap_or_default();
            let mut steps = Vec::new();
            for (i, part) in text.split("\\n").enumerate() {
                if i > 0 {
                    steps.push(macros::Step::Key(0x28));
                }
                if !part.is_empty() {
                    steps.push(macros::Step::Text(part.to_string()));
                }
            }
            all[n] = steps;
            kb.write_macros(&macros::encode_all(&all), size)?;
            println!("macro {n} saved");
        }
        Some("raw") => {
            // Bytes in hex, as the keyboard stores them (e.g. 01 02 e0 = press Ctrl).
            let n: usize = args.get(1).and_then(|a| a.parse().ok()).ok_or_else(|| anyhow::anyhow!("which macro?"))?;
            let bytes: Vec<u8> = args[2..].iter().map(|h| u8::from_str_radix(h, 16)).collect::<Result<_, _>>()?;
            all[n] = macros::decode(&bytes);
            kb.write_macros(&macros::encode_all(&all), size)?;
            println!("macro {n} saved: {:?}", all[n]);
        }
        Some("key") => {
            let n: u16 = args.get(1).and_then(|a| a.parse().ok()).ok_or_else(|| anyhow::anyhow!("which macro?"))?;
            let label = args.get(2).map(String::as_str).unwrap_or("");
            let key = profile.keys.iter().find(|k| k.label.eq_ignore_ascii_case(label)).ok_or_else(|| anyhow::anyhow!("no key {label}"))?;
            kb.set_key(profile.fn_layer as u8, key.row as u8, key.col as u8, macros::macro_keycode(n as usize))?;
            println!("Fn + {} runs macro {n}", key.label);
        }
        Some("play") => {
            let n: u8 = args.get(1).and_then(|a| a.parse().ok()).ok_or_else(|| anyhow::anyhow!("which macro?"))?;
            kb.set(CHANNEL_KEYBOARD, profile.macro_play_value, &[n])?;
        }
        Some(other) => anyhow::bail!("unknown: {other}"),
    }
    Ok(())
}

