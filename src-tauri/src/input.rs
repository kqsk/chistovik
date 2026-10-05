//! Keyboard chords via one SendInput batch (VK + scan code, explicit key-ups). No enigo,
//! no Unicode-typing flag (layout-independent).

use windows::Win32::UI::Input::KeyboardAndMouse::{
    MapVirtualKeyW, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, MAPVK_VK_TO_VSC, VIRTUAL_KEY,
};

fn key_input(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    let scan = unsafe { MapVirtualKeyW(vk.0 as u32, MAPVK_VK_TO_VSC) } as u16;
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: scan,
                dwFlags: if up {
                    KEYEVENTF_KEYUP
                } else {
                    KEYBD_EVENT_FLAGS(0)
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// modifiers down (in order), key down, key up, modifiers up (reverse order).
pub(crate) fn build_chord_inputs(modifiers: &[VIRTUAL_KEY], key: VIRTUAL_KEY) -> Vec<INPUT> {
    let mut v: Vec<INPUT> = modifiers.iter().map(|&m| key_input(m, false)).collect();
    v.push(key_input(key, false));
    v.push(key_input(key, true));
    v.extend(modifiers.iter().rev().map(|&m| key_input(m, true)));
    v
}

/// `modifier` held while an extended key (arrows) is pressed `times` times, one batch.
pub(crate) fn build_repeat_inputs(
    modifier: VIRTUAL_KEY,
    key: VIRTUAL_KEY,
    times: usize,
) -> Vec<INPUT> {
    let ext = |mut i: INPUT| {
        unsafe { i.Anonymous.ki.dwFlags |= KEYEVENTF_EXTENDEDKEY };
        i
    };
    let mut v = Vec::with_capacity(times * 2 + 2);
    v.push(key_input(modifier, false));
    for _ in 0..times {
        v.push(ext(key_input(key, false)));
        v.push(ext(key_input(key, true)));
    }
    v.push(key_input(modifier, true));
    v
}

fn send(inputs: &[INPUT]) -> Result<(), String> {
    let sent = unsafe { SendInput(inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize == inputs.len() {
        Ok(())
    } else {
        Err(format!(
            "SendInput injected {sent} of {} events",
            inputs.len()
        ))
    }
}

pub fn send_repeat(modifier: VIRTUAL_KEY, key: VIRTUAL_KEY, times: usize) -> Result<(), String> {
    send(&build_repeat_inputs(modifier, key, times))
}

pub fn send_chord(modifiers: &[VIRTUAL_KEY], key: VIRTUAL_KEY) -> Result<(), String> {
    send(&build_chord_inputs(modifiers, key))
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::Input::KeyboardAndMouse::{VK_C, VK_CONTROL, VK_LEFT, VK_SHIFT};

    #[test]
    fn repeat_holds_modifier_and_marks_arrows_extended() {
        let v = build_repeat_inputs(VK_SHIFT, VK_LEFT, 3);
        assert_eq!(v.len(), 8);
        assert_eq!(flags(&v[0]), (VK_SHIFT.0, false));
        assert_eq!(flags(&v[7]), (VK_SHIFT.0, true));
        for i in &v[1..7] {
            let f = unsafe { i.Anonymous.ki.dwFlags };
            assert_ne!(f.0 & KEYEVENTF_EXTENDEDKEY.0, 0);
        }
        assert_eq!(flags(&v[1]), (VK_LEFT.0, false));
        assert_eq!(flags(&v[2]), (VK_LEFT.0, true));
    }

    fn flags(i: &INPUT) -> (u16, bool) {
        unsafe {
            let ki = i.Anonymous.ki;
            (ki.wVk.0, ki.dwFlags.0 & KEYEVENTF_KEYUP.0 != 0)
        }
    }

    #[test]
    fn single_key_is_down_then_up() {
        let v = build_chord_inputs(&[], VK_C);
        assert_eq!(v.len(), 2);
        assert_eq!(flags(&v[0]), (VK_C.0, false));
        assert_eq!(flags(&v[1]), (VK_C.0, true));
    }

    #[test]
    fn ctrl_chord_order_and_scan() {
        let v = build_chord_inputs(&[VK_CONTROL], VK_C);
        let seq: Vec<_> = v.iter().map(flags).collect();
        assert_eq!(
            seq,
            vec![
                (VK_CONTROL.0, false),
                (VK_C.0, false),
                (VK_C.0, true),
                (VK_CONTROL.0, true)
            ]
        );
        assert_ne!(unsafe { v[1].Anonymous.ki.wScan }, 0);
    }
}
