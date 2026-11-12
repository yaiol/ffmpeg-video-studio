//! The colour the system paints its title bars with, when it paints them at all.
//!
//! Windows has two settings: an accent colour, and a switch "show the accent colour on title bars and window
//! borders". With the switch on, every title bar is that colour whatever the light or dark mode; with it off they are
//! neutral. This window draws its own bar, so it reads the two settings and does the same. Windows only: elsewhere
//! there is no such switch (a Mac's title bars are always neutral) and the answer is "none".

/// The system's title-bar colour as (red, green, blue), or `None` when title bars are not coloured.
#[cfg(windows)]
pub fn title_bar() -> Option<(u8, u8, u8)> {
    // Both values are in the window manager's own settings: the switch (1 = on) and the colour, stored blue-green-red.
    if dword("ColorPrevalence")? != 1 {
        return None;
    }
    let abgr = dword("AccentColor")?;
    Some(((abgr & 0xff) as u8, ((abgr >> 8) & 0xff) as u8, ((abgr >> 16) & 0xff) as u8))
}

#[cfg(not(windows))]
pub fn title_bar() -> Option<(u8, u8, u8)> {
    None
}

#[cfg(windows)]
fn dword(name: &str) -> Option<u32> {
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
    let wide = |s: &str| s.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let key = wide(r"Software\Microsoft\Windows\DWM");
    let value = wide(name);
    let mut data: u32 = 0;
    let mut size = std::mem::size_of::<u32>() as u32;
    // SAFETY: a read of one DWORD into a buffer of exactly that size; the strings are NUL-terminated.
    let status = unsafe {
        RegGetValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr(), RRF_RT_REG_DWORD, std::ptr::null_mut(), (&mut data as *mut u32).cast(), &mut size)
    };
    (status == 0).then_some(data)
}

/// The three values the window bar takes for a title-bar colour: its background, the text and icons on it (light
/// on a dark colour, dark on a light one) and a button's background under the pointer.
pub fn bar_colours((r, g, b): (u8, u8, u8)) -> (String, &'static str, &'static str) {
    // Perceived brightness (the usual weights for red, green and blue), 0 = black, 255 = white.
    let brightness = (299 * r as u32 + 587 * g as u32 + 114 * b as u32) / 1000;
    let (text, hover) = if brightness < 150 { ("#ffffff", "rgba(255,255,255,0.18)") } else { ("#1a1a1a", "rgba(0,0,0,0.12)") };
    (format!("#{r:02x}{g:02x}{b:02x}"), text, hover)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_dark_colour_gets_light_text_and_a_light_one_dark_text() {
        assert_eq!(bar_colours((0x10, 0x89, 0x3e)), ("#10893e".to_owned(), "#ffffff", "rgba(255,255,255,0.18)"));
        assert_eq!(bar_colours((0xff, 0xb9, 0x00)).1, "#1a1a1a");
        assert_eq!(bar_colours((0, 0, 0)).1, "#ffffff");
        assert_eq!(bar_colours((255, 255, 255)).1, "#1a1a1a");
    }
}
