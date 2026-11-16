//! RK71 Configurator: settings and firmware updates for the RK71 QMK firmware.
//!
//!   rk71-configurator            open the app
//!   rk71-configurator --probe    print every keyboard setting in the terminal (USB test)

mod app;
mod device;

use anyhow::Result;
use device::{Keyboard, CHANNEL_KEYBOARD, CHANNEL_RGB_MATRIX};

fn main() -> Result<()> {
    if std::env::args().any(|a| a == "--probe") {
        return probe();
    }
    app::run();
    Ok(())
}

/// Command-line USB test: read and print every setting.
fn probe() -> Result<()> {
    let kb = Keyboard::open()?;
    println!("Connected. VIA protocol version {}", kb.protocol_version);
    println!(
        "Key lighting: brightness {:?} effect {:?} speed {:?} colour (hue, sat) {:?}",
        kb.get(CHANNEL_RGB_MATRIX, 1, 1)?,
        kb.get(CHANNEL_RGB_MATRIX, 2, 1)?,
        kb.get(CHANNEL_RGB_MATRIX, 3, 1)?,
        kb.get(CHANNEL_RGB_MATRIX, 4, 2)?
    );
    for id in 1..=49u8 {
        match kb.get(CHANNEL_KEYBOARD, id, 2) {
            Ok(v) => println!("  keyboard value {id:2}: {v:?}"),
            Err(e) => println!("  keyboard value {id:2}: ({e})"),
        }
    }
    Ok(())
}
