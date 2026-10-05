//! Decisions for "is it still safe to paste over what was captured?". Pure functions only;
//! the Win32 / UIA glue lives in `bg_insert` and `capture`. Text is never logged.

/// What to do with a classic Edit / RichEdit control right before pasting.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EditAction {
    /// Selection is exactly the captured one.
    Proceed,
    /// Selection moved, but the captured range still holds the captured text: re-select it.
    Reselect,
    /// Cannot prove the original text is where it was.
    Fallback,
}

/// `text_at_range`: Some(true/false) when the text at the captured range could be read and
/// did / did not equal the captured text; None when it could not be read.
pub fn edit_action(
    captured: (u32, u32),
    current: Option<(u32, u32)>,
    text_at_range: Option<bool>,
) -> EditAction {
    if captured.0 >= captured.1 {
        return EditAction::Fallback;
    }
    if current == Some(captured) {
        // Unreadable text (e.g. RichEditD2DPT across processes): the range check alone decides.
        return match text_at_range {
            Some(false) => EditAction::Fallback,
            _ => EditAction::Proceed,
        };
    }
    match text_at_range {
        Some(true) => EditAction::Reselect,
        _ => EditAction::Fallback,
    }
}

/// What to do with a UIA control whose selection differs from the captured text.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FindAction {
    Select,
    Fallback,
}

/// Exactly one occurrence of the captured text is unambiguous; zero or many is not.
pub fn find_action(matches: usize) -> FindAction {
    if matches == 1 {
        FindAction::Select
    } else {
        FindAction::Fallback
    }
}

/// Classic-edit verification could not vouch for the selection: should the UIA verifier get a
/// turn? Only when the text was captured through UIA (so UIA can find / re-select it).
pub fn try_uia_after_classic(classic_ok: bool, uia_selection_capture: bool) -> bool {
    !classic_ok && uia_selection_capture
}

/// Text equality tolerant to newline forms and surrounding whitespace (controls report the
/// same content with CR, LF or CRLF, and sometimes a trailing newline).
pub fn texts_equal(a: &str, b: &str) -> bool {
    fn norm(s: &str) -> String {
        s.replace("\r\n", "\n").replace('\r', "\n")
    }
    norm(a).trim() == norm(b).trim()
}

/// Slice of UTF-16 units `[start, end)` of `all`, as a string (None when out of bounds).
pub fn utf16_slice(all: &[u16], start: u32, end: u32) -> Option<String> {
    let (s, e) = (start as usize, end as usize);
    (s <= e && e <= all.len()).then(|| String::from_utf16_lossy(&all[s..e]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_same_range() {
        assert_eq!(
            edit_action((2, 9), Some((2, 9)), Some(true)),
            EditAction::Proceed
        );
        assert_eq!(edit_action((2, 9), Some((2, 9)), None), EditAction::Proceed);
        assert_eq!(
            edit_action((2, 9), Some((2, 9)), Some(false)),
            EditAction::Fallback
        );
    }

    #[test]
    fn edit_moved_range() {
        assert_eq!(
            edit_action((2, 9), Some((5, 5)), Some(true)),
            EditAction::Reselect
        );
        assert_eq!(
            edit_action((2, 9), Some((5, 5)), Some(false)),
            EditAction::Fallback
        );
        assert_eq!(
            edit_action((2, 9), Some((5, 5)), None),
            EditAction::Fallback
        );
        assert_eq!(edit_action((2, 9), None, Some(true)), EditAction::Reselect);
    }

    #[test]
    fn edit_empty_captured_never_proceeds() {
        assert_eq!(
            edit_action((4, 4), Some((4, 4)), Some(true)),
            EditAction::Fallback
        );
    }

    #[test]
    fn uia_fallthrough_decision() {
        assert!(try_uia_after_classic(false, true));
        assert!(!try_uia_after_classic(true, true));
        assert!(!try_uia_after_classic(false, false));
        assert!(!try_uia_after_classic(true, false));
    }

    #[test]
    fn find_requires_exactly_one() {
        assert_eq!(find_action(0), FindAction::Fallback);
        assert_eq!(find_action(1), FindAction::Select);
        assert_eq!(find_action(2), FindAction::Fallback);
    }

    #[test]
    fn text_equality_tolerates_newlines_and_edges() {
        assert!(texts_equal("a\r\nb", "a\nb"));
        assert!(texts_equal("a\rb", "a\r\nb"));
        assert!(texts_equal("abc\r\n", "abc"));
        assert!(!texts_equal("abc", "abd"));
    }

    #[test]
    fn slice_bounds() {
        let w: Vec<u16> = "привет мир".encode_utf16().collect();
        assert_eq!(utf16_slice(&w, 7, 10).as_deref(), Some("мир"));
        assert_eq!(utf16_slice(&w, 7, 11), None);
        assert_eq!(utf16_slice(&w, 5, 2), None);
    }
}
