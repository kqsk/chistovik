use serde::Serialize;

#[derive(Serialize, Clone, Debug)]
pub struct ThemeInfo {
    /// "#RRGGBB"
    pub accent: String,
    /// true -> no transparency effects, draw an opaque surface
    pub opaque: bool,
    pub dark: bool,
}

impl Default for ThemeInfo {
    fn default() -> Self {
        ThemeInfo {
            accent: "#0067C0".into(),
            opaque: false,
            dark: false,
        }
    }
}

/// Windows stores the DWM accent as ABGR (0xAABBGGRR). Returns "#RRGGBB".
pub fn abgr_to_hex(v: u32) -> String {
    let r = v & 0xFF;
    let g = (v >> 8) & 0xFF;
    let b = (v >> 16) & 0xFF;
    format!("#{r:02X}{g:02X}{b:02X}")
}

#[cfg(windows)]
mod win {
    use windows::core::{w, PCWSTR};
    use windows::Win32::System::Registry::{
        RegGetValueW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RRF_RT_REG_DWORD, RRF_RT_REG_SZ,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_REMOTESESSION};

    fn read_dword(root: HKEY, subkey: PCWSTR, value: PCWSTR) -> Option<u32> {
        let mut data: u32 = 0;
        let mut size = std::mem::size_of::<u32>() as u32;
        let rc = unsafe {
            RegGetValueW(
                root,
                subkey,
                value,
                RRF_RT_REG_DWORD,
                None,
                Some(&mut data as *mut u32 as *mut _),
                Some(&mut size),
            )
        };
        if rc.0 == 0 {
            Some(data)
        } else {
            None
        }
    }

    fn read_build_number() -> Option<u32> {
        let mut buf = [0u16; 32];
        let mut size = (buf.len() * 2) as u32;
        let rc = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                w!("SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion"),
                w!("CurrentBuildNumber"),
                RRF_RT_REG_SZ,
                None,
                Some(buf.as_mut_ptr() as *mut _),
                Some(&mut size),
            )
        };
        if rc.0 != 0 {
            return None;
        }
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..len]).trim().parse().ok()
    }

    pub fn read_theme() -> super::ThemeInfo {
        let dark = read_dword(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("AppsUseLightTheme"),
        ) == Some(0);

        let accent = read_dword(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\DWM"),
            w!("AccentColor"),
        )
        .map(super::abgr_to_hex)
        .unwrap_or_else(|| {
            if dark {
                "#60CDFF".into()
            } else {
                "#0067C0".into()
            }
        });

        let transparency_off = read_dword(
            HKEY_CURRENT_USER,
            w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize"),
            w!("EnableTransparency"),
        ) == Some(0);
        let remote = unsafe { GetSystemMetrics(SM_REMOTESESSION) } != 0;
        let old_build = read_build_number().is_some_and(|b| b < 17763);

        super::ThemeInfo {
            accent,
            opaque: transparency_off || remote || old_build,
            dark,
        }
    }
}

#[cfg(windows)]
pub fn read_theme() -> ThemeInfo {
    win::read_theme()
}

#[cfg(not(windows))]
pub fn read_theme() -> ThemeInfo {
    ThemeInfo::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abgr_converts_to_rgb_hex() {
        // A=FF B=C0 G=67 R=00 -> #0067C0
        assert_eq!(abgr_to_hex(0xFFC0_6700), "#0067C0");
        assert_eq!(abgr_to_hex(0xFF00_00FF), "#FF0000");
    }
}
