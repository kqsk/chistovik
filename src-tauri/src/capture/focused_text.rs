//! UI Automation reading of the focused control (selection or whole value) and password /
//! editable detection. Read and select only: the result is written back through Ctrl+V.
//!
//! Derived from Handy (MIT, Copyright (c) 2025 CJ Pais) -- see NOTICE.
//! Text is never logged.

use crate::selection_guard::{find_action, texts_equal, FindAction};
use windows::core::{BOOL, BSTR};
use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED,
};
use windows::Win32::UI::Accessibility::{
    CUIAutomation, IUIAutomation, IUIAutomationElement, IUIAutomationTextPattern,
    IUIAutomationTextRange, IUIAutomationValuePattern, TextPatternRangeEndpoint_End,
    TextPatternRangeEndpoint_Start, UIA_EditControlTypeId, UIA_TextEditPatternId,
    UIA_TextPatternId, UIA_ValuePatternId,
};

/// Initializes COM (MTA) on the current thread for the guard's lifetime.
pub(crate) struct ComGuard(bool);

impl ComGuard {
    pub(crate) fn new() -> Self {
        // S_FALSE (already initialized) still needs a matching CoUninitialize.
        Self(unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }.is_ok())
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.0 {
            unsafe { CoUninitialize() };
        }
    }
}

pub(crate) struct UiaRead {
    pub text: String,
    /// True when `text` is the user's selection, false when it is the whole field.
    pub is_selection: bool,
    pub editable: bool,
    // Part of the read contract; consumed by diagnostics/tests only for now.
    #[allow(dead_code)]
    pub has_text_pattern: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum UiaError {
    NoElement,
    Password,
    /// Not editable and nothing selected.
    NotEditable,
    /// Editable control (Edit/TextEdit) that exposes no readable text: Ctrl+A/Ctrl+C may work.
    EditableNoText,
    Empty,
}

fn automation() -> Option<IUIAutomation> {
    unsafe { CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) }.ok()
}

fn is_true(value: windows::core::Result<BOOL>) -> bool {
    value.map(|b| b.as_bool()).unwrap_or(false)
}

fn range_text(range: &IUIAutomationTextRange) -> Option<String> {
    unsafe { range.GetText(-1) }.ok().map(|s| s.to_string())
}

/// First non-empty selection text of a text pattern, if any.
fn selection_text(pattern: &IUIAutomationTextPattern) -> Option<String> {
    let ranges = unsafe { pattern.GetSelection() }.ok()?;
    let len = unsafe { ranges.Length() }.ok()?;
    (0..len).find_map(|i| {
        let range = unsafe { ranges.GetElement(i) }.ok()?;
        let text = range_text(&range)?;
        (!text.is_empty()).then_some(text)
    })
}

fn focused_element(a: &IUIAutomation) -> Option<IUIAutomationElement> {
    unsafe { a.GetFocusedElement() }.ok()
}

/// Reads the focused element: the selection if there is one, else (editable only) the whole
/// value. Must run on a COM-initialised (MTA) thread.
pub(crate) fn read_focused() -> Result<UiaRead, UiaError> {
    let a = automation().ok_or(UiaError::NoElement)?;
    let element = focused_element(&a).ok_or(UiaError::NoElement)?;

    if is_true(unsafe { element.CurrentIsPassword() }) {
        return Err(UiaError::Password);
    }

    let value_pattern =
        unsafe { element.GetCurrentPatternAs::<IUIAutomationValuePattern>(UIA_ValuePatternId) }
            .ok();
    let text_pattern =
        unsafe { element.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId) }.ok();

    let value_read_only = value_pattern
        .as_ref()
        .is_some_and(|p| is_true(unsafe { p.CurrentIsReadOnly() }));
    let editable = !value_read_only
        && (value_pattern.is_some()
            || unsafe { element.CurrentControlType() }.ok() == Some(UIA_EditControlTypeId)
            || unsafe { element.GetCurrentPattern(UIA_TextEditPatternId) }.is_ok());
    let has_text_pattern = text_pattern.is_some();

    // A selection is read even from non-editable documents (CAPT-01).
    if let Some(selected) = text_pattern.as_ref().and_then(selection_text) {
        return Ok(UiaRead {
            text: selected,
            is_selection: true,
            editable,
            has_text_pattern,
        });
    }
    if !editable {
        return Err(UiaError::NotEditable);
    }

    let value = value_pattern
        .as_ref()
        .and_then(|p| unsafe { p.CurrentValue() }.ok())
        .map(|v| v.to_string());
    let text = match value {
        Some(v) => Some(v),
        None => text_pattern
            .as_ref()
            .and_then(|tp| unsafe { tp.DocumentRange() }.ok())
            .and_then(|r| range_text(&r)),
    };
    match text {
        Some(t) if t.trim().is_empty() => Err(UiaError::Empty),
        Some(t) => Ok(UiaRead {
            text: t,
            is_selection: false,
            editable,
            has_text_pattern,
        }),
        None => Err(UiaError::EditableNoText),
    }
}

/// Fast check on the currently focused element only.
#[allow(dead_code)]
pub(crate) fn is_password_focused() -> Option<bool> {
    let a = automation()?;
    let e = focused_element(&a)?;
    Some(is_true(unsafe { e.CurrentIsPassword() }))
}

/// Best-effort: password flag of the element behind a window handle. In Chrome/Electron this
/// is the renderer document, not the focused `<input>`, so it reports false there.
pub(crate) fn is_password_hwnd(hwnd: isize) -> Option<bool> {
    let a = automation()?;
    let e = unsafe { a.ElementFromHandle(HWND(hwnd as *mut _)) }.ok()?;
    Some(is_true(unsafe { e.CurrentIsPassword() }))
}

/// Selects the whole document of the focused control through TextPattern. True on success.
pub(crate) fn select_all_focused() -> bool {
    let Some(a) = automation() else {
        return false;
    };
    let Some(e) = focused_element(&a) else {
        return false;
    };
    let Ok(tp) = (unsafe { e.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId) })
    else {
        return false;
    };
    let Ok(doc) = (unsafe { tp.DocumentRange() }) else {
        return false;
    };
    unsafe { doc.Select() }.is_ok()
}

/// Text pattern of the focused element.
fn focused_text_pattern() -> Option<IUIAutomationTextPattern> {
    let a = automation()?;
    let e = focused_element(&a)?;
    unsafe { e.GetCurrentPatternAs::<IUIAutomationTextPattern>(UIA_TextPatternId) }.ok()
}

/// Screen rect (left, top, right, bottom; physical px) of the caret / selection start in the
/// focused control. A collapsed caret often has no rectangles, so it is widened to one
/// character. Degenerate rects are rejected.
pub(crate) fn caret_rect() -> Option<(i32, i32, i32, i32)> {
    use windows::Win32::System::Ole::{
        SafeArrayAccessData, SafeArrayDestroy, SafeArrayGetUBound, SafeArrayUnaccessData,
    };
    use windows::Win32::UI::Accessibility::TextUnit_Character;

    fn first_rect(range: &IUIAutomationTextRange) -> Option<(i32, i32, i32, i32)> {
        unsafe {
            let sa = range.GetBoundingRectangles().ok()?;
            if sa.is_null() {
                return None;
            }
            let mut out = None;
            // Flat array of doubles: left, top, width, height per line.
            if SafeArrayGetUBound(sa, 1).is_ok_and(|ub| ub >= 3) {
                let mut data: *mut std::ffi::c_void = std::ptr::null_mut();
                if SafeArrayAccessData(sa, &mut data).is_ok() {
                    let v = std::slice::from_raw_parts(data as *const f64, 4);
                    if v[3] >= 1.0 {
                        let (l, t) = (v[0].round() as i32, v[1].round() as i32);
                        out = Some((l, t, l + v[2].round() as i32, t + v[3].round() as i32));
                    }
                    let _ = SafeArrayUnaccessData(sa);
                }
            }
            let _ = SafeArrayDestroy(sa);
            out
        }
    }

    let pattern = focused_text_pattern()?;
    let ranges = unsafe { pattern.GetSelection() }.ok()?;
    if unsafe { ranges.Length() }.ok()? < 1 {
        return None;
    }
    let range = unsafe { ranges.GetElement(0) }.ok()?;
    first_rect(&range).or_else(|| {
        let wide = unsafe { range.Clone() }.ok()?;
        unsafe { wide.ExpandToEnclosingUnit(TextUnit_Character) }.ok()?;
        first_rect(&wide)
    })
}

/// Selection text of the focused element (None = no selection / unreadable).
pub(crate) fn current_selection_text() -> Option<String> {
    selection_text(&focused_text_pattern()?)
}

/// Makes sure the focused element's selection is `captured`: already so, or exactly one
/// occurrence of it in the document gets selected (and confirmed by a re-read).
pub(crate) fn ensure_selection(captured: &str) -> bool {
    let Some(tp) = focused_text_pattern() else {
        return false;
    };
    if selection_text(&tp).is_some_and(|t| texts_equal(&t, captured)) {
        return true;
    }
    let Ok(doc) = (unsafe { tp.DocumentRange() }) else {
        return false;
    };
    let needle = BSTR::from(captured);
    let Ok(first) = (unsafe { doc.FindText(&needle, false, false) }) else {
        return false;
    };
    // Look for a second occurrence after the first one.
    let more = unsafe { doc.Clone() }
        .ok()
        .filter(|rest| {
            unsafe {
                rest.MoveEndpointByRange(
                    TextPatternRangeEndpoint_Start,
                    &first,
                    TextPatternRangeEndpoint_End,
                )
            }
            .is_ok()
        })
        .map(|rest| usize::from(unsafe { rest.FindText(&needle, false, false) }.is_ok()))
        .unwrap_or(1); // cannot count -> treat as ambiguous
    if find_action(1 + more) != FindAction::Select {
        return false;
    }
    if unsafe { first.Select() }.is_err() {
        return false;
    }
    selection_text(&tp).is_some_and(|t| texts_equal(&t, captured))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn com_guard_nests() {
        let _a = ComGuard::new();
        let _b = ComGuard::new();
    }

    #[test]
    #[ignore = "interactive: needs a focused edit control (Notepad)"]
    fn reads_focused_notepad() {
        let _g = ComGuard::new();
        let r = read_focused();
        eprintln!("read_focused ok={}", r.is_ok());
    }
}
