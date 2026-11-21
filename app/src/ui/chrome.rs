//! The window's material on macOS: the sidebar uses the system's sidebar material, edge to edge;
//! on macOS 26 (Tahoe) it also floats as a Liquid Glass pane, as that release's sidebars do.
//! Elsewhere the window is opaque. Also the AppKit calls GPUI doesn't wrap: the window's
//! light/dark appearance and the standard About panel.

use gpui::{div, prelude::*, px, Div, Hsla, MouseButton, Pixels, Point, Window, WindowBackgroundAppearance};

#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))] // only macOS is translucent
pub enum Backdrop {
    Opaque,
    /// The system's sidebar material, flush with the window's edges.
    Sidebar,
    /// macOS 26: a floating Liquid Glass pane over the sidebar material.
    FloatingGlass,
}

/// Gap between the floating pane and the window's edges.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const PANE_INSET: f64 = 8.0;
/// Concentric with the window's corners.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const PANE_RADIUS: f64 = 18.0;

/// The strip our content leaves for a title bar that's only drawn by the system. A double click does
/// what the user chose for title bars (System Settings: zoom, by default, or minimize).
pub fn title_strip(height: Pixels) -> Div {
    div().flex_none().w_full().h(height).on_mouse_down(MouseButton::Left, |event, window, _| {
        if event.click_count == 2 {
            window.titlebar_double_click();
        }
    })
}

impl Backdrop {
    pub fn detect() -> Self {
        #[cfg(target_os = "macos")]
        {
            if mac::is_tahoe() && mac::has_glass() {
                Backdrop::FloatingGlass
            } else {
                Backdrop::Sidebar
            }
        }
        #[cfg(not(target_os = "macos"))]
        Backdrop::Opaque
    }

    /// The content runs under the title bar, and the sidebar lets the material through.
    pub fn is_translucent(self) -> bool {
        self != Backdrop::Opaque
    }

    /// GPUI only has to let the window show through; the material is our own view.
    pub fn window_background(self) -> WindowBackgroundAppearance {
        if self.is_translucent() {
            WindowBackgroundAppearance::Transparent
        } else {
            WindowBackgroundAppearance::Opaque
        }
    }

    /// The close button's spot, inside the sidebar (and inside the floating pane).
    pub fn traffic_lights(self) -> Option<Point<Pixels>> {
        match self {
            Backdrop::Opaque => None,
            Backdrop::Sidebar => Some(gpui::point(px(18.), px(18.))),
            Backdrop::FloatingGlass => Some(gpui::point(px(22.), px(22.))),
        }
    }

    /// Width of the icons-only sidebar. On macOS the window buttons sit in it and have to fit, with
    /// the same margin on both sides.
    pub fn rail_width(self) -> f32 {
        match self {
            Backdrop::Opaque => 48.,
            Backdrop::Sidebar => 96.,
            Backdrop::FloatingGlass => 104.,
        }
    }

    /// Space between the sidebar's edge and its rows. With the rows' own padding, their icons line
    /// up with the window buttons.
    pub fn sidebar_inset(self) -> f32 {
        match self {
            Backdrop::Opaque => 8.,
            Backdrop::Sidebar => 10.,
            Backdrop::FloatingGlass => 14.,
        }
    }

    /// Height of the strip at the top that moves the window (the title bar's place).
    pub fn title_bar_height(self) -> Pixels {
        match self {
            Backdrop::Opaque => px(0.),
            Backdrop::Sidebar => px(44.),
            Backdrop::FloatingGlass => px(52.),
        }
    }
}

/// The native views behind the sidebar; they follow its width.
pub struct SidebarViews {
    #[cfg(target_os = "macos")]
    material: objc2::rc::Retained<objc2::runtime::AnyObject>,
    #[cfg(target_os = "macos")]
    glass: Option<objc2::rc::Retained<objc2::runtime::AnyObject>>,
    width: f32,
}

impl SidebarViews {
    /// Put the material (and the glass pane, on macOS 26) under a sidebar `width` wide.
    pub fn install(backdrop: Backdrop, window: &Window, width: f32) -> Option<SidebarViews> {
        #[cfg(target_os = "macos")]
        {
            if !backdrop.is_translucent() {
                return None;
            }
            let material = mac::sidebar_material(window, width as f64)?;
            let glass = (backdrop == Backdrop::FloatingGlass)
                .then(|| mac::glass_pane(window, width as f64, PANE_INSET, PANE_RADIUS))
                .flatten();
            Some(SidebarViews { material, glass, width })
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (backdrop, window, width);
            None
        }
    }

    pub fn set_width(&mut self, width: f32) {
        if self.width != width {
            self.width = width;
            #[cfg(target_os = "macos")]
            {
                mac::resize(&self.material, width as f64, 0.0);
                if let Some(glass) = &self.glass {
                    mac::resize(glass, width as f64, PANE_INSET);
                }
            }
        }
    }
}

/// Make the window (and its materials) light or dark regardless of the system; None follows it.
pub fn set_dark(window: &Window, dark: Option<bool>) {
    #[cfg(target_os = "macos")]
    mac::set_appearance(window, dark);
    #[cfg(not(target_os = "macos"))]
    let _ = (window, dark);
}

/// The accent color chosen in the system's settings (macOS, Windows). `dark`: the app's theme, for
/// Windows, which has lighter and darker shades of it for each.
pub fn accent_color(dark: bool) -> Option<Hsla> {
    #[cfg(target_os = "macos")]
    return { let _ = dark; mac::accent_color() };
    #[cfg(windows)]
    return win::accent_color(dark);
    #[cfg(not(any(target_os = "macos", windows)))]
    { let _ = dark; None }
}

#[cfg(windows)]
mod win {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_BINARY};

    /// From the accent palette Windows keeps in the registry: 8 shades, lightest first, 4 bytes each
    /// (R, G, B, unused); the accent itself is the 4th. Its own controls use a lighter shade on dark
    /// backgrounds and a darker one on light ones (AccentFillColorDefault).
    pub fn accent_color(dark: bool) -> Option<gpui::Hsla> {
        let wide = |s: &str| s.encode_utf16().chain([0]).collect::<Vec<u16>>();
        let (key, name) = (wide(r"Software\Microsoft\Windows\CurrentVersion\Explorer\Accent"), wide("AccentPalette"));
        let mut palette = [0u8; 32];
        let mut len = palette.len() as u32;
        // SAFETY: the strings are NUL-terminated and the buffer's size is passed with it.
        let status = unsafe {
            RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr(), RRF_RT_REG_BINARY, std::ptr::null_mut(), palette.as_mut_ptr().cast(), &mut len)
        };
        if status != 0 || len < 32 {
            return None;
        }
        let shade = if dark { 1 } else { 4 };
        let [r, g, b] = [0, 1, 2].map(|i| palette[shade * 4 + i] as f32 / 255.0);
        Some(gpui::Rgba { r, g, b, a: 1.0 }.into())
    }
}

/// The standard About panel (macOS); the version comes from the build.
pub fn about() {
    #[cfg(target_os = "macos")]
    mac::about(&crate::tr!("app.name"), env!("CARGO_PKG_VERSION"), &crate::tr!("app.copyright_about", copyright = crate::COPYRIGHT));
}

#[cfg(target_os = "macos")]
mod mac {
    use std::ptr;

    use gpui::Window;
    use objc2::rc::Retained;
    use objc2::runtime::{AnyClass, AnyObject};
    use objc2::{class, msg_send};
    use objc2_foundation::{NSDictionary, NSPoint, NSProcessInfo, NSRect, NSSize, NSString};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    const NS_VIEW_MAX_X_MARGIN: usize = 1 << 2;
    const NS_VIEW_HEIGHT_SIZABLE: usize = 1 << 4;
    const NS_WINDOW_BELOW: isize = -1;
    const NS_VISUAL_EFFECT_MATERIAL_SIDEBAR: isize = 7;
    const NS_VISUAL_EFFECT_BLENDING_BEHIND_WINDOW: isize = 0;
    const NS_VISUAL_EFFECT_STATE_FOLLOWS_WINDOW: isize = 0;

    /// macOS 26. Apps built against an older SDK are told 16 instead.
    pub fn is_tahoe() -> bool {
        let major = NSProcessInfo::processInfo().operatingSystemVersion().majorVersion;
        major == 26 || major == 16
    }

    pub fn has_glass() -> bool {
        AnyClass::get(c"NSGlassEffectView").is_some()
    }

    /// GPUI's own view (it draws the whole window) and its NSWindow.
    fn views(window: &Window) -> Option<(*mut AnyObject, *mut AnyObject)> {
        let RawWindowHandle::AppKit(handle) = HasWindowHandle::window_handle(window).ok()?.as_raw() else { return None };
        let view: *mut AnyObject = handle.ns_view.as_ptr().cast();
        // SAFETY: GPUI's view is a live NSView for as long as `window` is.
        let ns_window: *mut AnyObject = unsafe { msg_send![view, window] };
        (!ns_window.is_null()).then_some((view, ns_window))
    }

    fn frame(content: *mut AnyObject, width: f64, inset: f64) -> NSRect {
        // SAFETY: `content` is the window's content view.
        let bounds: NSRect = unsafe { msg_send![content, bounds] };
        NSRect::new(
            NSPoint::new(inset, inset),
            NSSize::new((width - 2.0 * inset).max(0.0), (bounds.size.height - 2.0 * inset).max(0.0)),
        )
    }

    /// Add `view` to the content view right under GPUI's (so above anything added before),
    /// pinned to the left edge and as tall as the window.
    fn insert(window: &Window, view: &AnyObject, width: f64, inset: f64) -> Option<()> {
        let (gpui_view, ns_window) = views(window)?;
        // SAFETY: AppKit calls on the main thread with live objects.
        unsafe {
            let content: *mut AnyObject = msg_send![ns_window, contentView];
            let _: () = msg_send![view, setFrame: frame(content, width, inset)];
            let _: () = msg_send![view, setAutoresizingMask: NS_VIEW_HEIGHT_SIZABLE | NS_VIEW_MAX_X_MARGIN];
            let _: () = msg_send![content, addSubview: view, positioned: NS_WINDOW_BELOW, relativeTo: gpui_view];
        }
        Some(())
    }

    pub fn sidebar_material(window: &Window, width: f64) -> Option<Retained<AnyObject>> {
        // SAFETY: AppKit calls on the main thread.
        let view: Retained<AnyObject> = unsafe {
            let view: Retained<AnyObject> = msg_send![class!(NSVisualEffectView), new];
            let _: () = msg_send![&*view, setMaterial: NS_VISUAL_EFFECT_MATERIAL_SIDEBAR];
            let _: () = msg_send![&*view, setBlendingMode: NS_VISUAL_EFFECT_BLENDING_BEHIND_WINDOW];
            let _: () = msg_send![&*view, setState: NS_VISUAL_EFFECT_STATE_FOLLOWS_WINDOW];
            view
        };
        insert(window, &view, width, 0.0)?;
        Some(view)
    }

    pub fn glass_pane(window: &Window, width: f64, inset: f64, radius: f64) -> Option<Retained<AnyObject>> {
        let class = AnyClass::get(c"NSGlassEffectView")?;
        // SAFETY: AppKit calls on the main thread.
        let pane: Retained<AnyObject> = unsafe {
            let pane: Retained<AnyObject> = msg_send![class, new];
            let _: () = msg_send![&*pane, setCornerRadius: radius];
            pane
        };
        insert(window, &pane, width, inset)?;
        Some(pane)
    }

    pub fn resize(view: &AnyObject, width: f64, inset: f64) {
        // SAFETY: the view is alive (retained) and still in its content view.
        unsafe {
            let content: *mut AnyObject = msg_send![view, superview];
            if !content.is_null() {
                let _: () = msg_send![view, setFrame: frame(content, width, inset)];
            }
        }
    }

    pub fn accent_color() -> Option<gpui::Hsla> {
        // SAFETY: AppKit calls on the main thread; the color is checked before it's read.
        unsafe {
            let accent: *mut AnyObject = msg_send![class!(NSColor), controlAccentColor];
            let srgb: *mut AnyObject = msg_send![class!(NSColorSpace), sRGBColorSpace];
            let color: *mut AnyObject = msg_send![accent, colorUsingColorSpace: srgb];
            if color.is_null() {
                return None;
            }
            let (r, g, b): (f64, f64, f64) =
                (msg_send![color, redComponent], msg_send![color, greenComponent], msg_send![color, blueComponent]);
            Some(gpui::Rgba { r: r as f32, g: g as f32, b: b as f32, a: 1.0 }.into())
        }
    }

    pub fn set_appearance(window: &Window, dark: Option<bool>) {
        let Some((_, ns_window)) = views(window) else { return };
        // SAFETY: AppKit calls on the main thread; a nil appearance means "follow the system".
        unsafe {
            let appearance: *mut AnyObject = match dark {
                None => ptr::null_mut(),
                Some(dark) => {
                    let name = NSString::from_str(if dark { "NSAppearanceNameDarkAqua" } else { "NSAppearanceNameAqua" });
                    msg_send![class!(NSAppearance), appearanceNamed: &*name]
                }
            };
            let _: () = msg_send![ns_window, setAppearance: appearance];
        }
    }

    pub fn about(name: &str, version: &str, copyright: &str) {
        let keys = [
            NSString::from_str("ApplicationName"),
            NSString::from_str("ApplicationVersion"),
            NSString::from_str("Version"),
            NSString::from_str("Copyright"),
        ];
        let values = [NSString::from_str(name), NSString::from_str(version), NSString::from_str(version), NSString::from_str(copyright)];
        let options = NSDictionary::from_retained_objects(&keys.each_ref().map(|k| &**k), &values);
        // SAFETY: AppKit calls on the main thread.
        unsafe {
            let app: *mut AnyObject = msg_send![class!(NSApplication), sharedApplication];
            let _: () = msg_send![app, activateIgnoringOtherApps: true];
            let _: () = msg_send![app, orderFrontStandardAboutPanelWithOptions: &*options];
        }
    }
}
