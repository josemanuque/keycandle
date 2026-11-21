//! Embeds the app icon in the Windows executable (resource id 1, which GPUI uses for the window
//! and the taskbar). macOS and Linux take the icon from the app bundle / .desktop entry instead
//! (assets/icon/*.icns and png/).

fn main() {
    println!("cargo:rerun-if-changed=assets/icon/icon-light.ico");
    // The translations are compiled in by a macro (rust_i18n::i18n!), which cargo doesn't watch.
    println!("cargo:rerun-if-changed=locales");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("assets/icon/icon-light.ico");
        res.set("ProductName", "Keycandle");
        res.set("FileDescription", "Keycandle");
        res.set("LegalCopyright", "© 2026 josemanuque. GPL-2.0-or-later."); // as crate::COPYRIGHT
        res.compile().expect("could not embed the Windows icon");
    }
}
