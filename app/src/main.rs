//! RK71 Configurator.
//!
//! Stage 1 (now): command-line probe of the keyboard, to verify USB access on every OS.
//!   rk71-configurator --probe      read and print every keyboard setting
//! The GPUI window comes next.

mod device;

use anyhow::Result;
use device::{Keyboard, CHANNEL_KEYBOARD, CHANNEL_RGB_MATRIX};

fn main() -> Result<()> {
    let probe = std::env::args().any(|a| a == "--probe");
    if !probe {
        println!("RK71 Configurator: the window is not built yet. Run with --probe to test the connection.");
        return Ok(());
    }
    let kb = Keyboard::open()?;
    println!("Connected. VIA protocol version {}", kb.protocol_version);

    println!("Key lighting: brightness {:?} effect {:?} speed {:?} colour (hue, sat) {:?}",
        kb.get(CHANNEL_RGB_MATRIX, 1, 1)?, kb.get(CHANNEL_RGB_MATRIX, 2, 1)?,
        kb.get(CHANNEL_RGB_MATRIX, 3, 1)?, kb.get(CHANNEL_RGB_MATRIX, 4, 2)?);
    for id in 1..=49u8 {
        match kb.get(CHANNEL_KEYBOARD, id, 2) {
            Ok(v) => println!("  keyboard value {id:2}: {v:?}"),
            Err(e) => println!("  keyboard value {id:2}: ({e})"),
        }
    }
    Ok(())
}
