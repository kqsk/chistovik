//! Clipboard helpers: text read/write plus a full multi-format snapshot/restore
//! (images, files, RTF/HTML, private formats) so the user's clipboard survives our work.
//!
//! Snapshot/restore logic derived from Handy (MIT, Copyright (c) 2025 CJ Pais) - see NOTICE.

pub(crate) mod paste_tx;
mod tx_state;

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;
use windows::core::w;
use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, EnumClipboardFormats, GetClipboardData,
    GetClipboardSequenceNumber, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE,
};

const CF_UNICODETEXT: u32 = 13;
/// Skip-or-fail threshold for a single format (as in Handy).
const MAX_FORMAT_BYTES: usize = 64 * 1024 * 1024;

/// Sequence number right after our last `write_text` (0 = none / cleared). `restore` refuses to
/// overwrite the clipboard when somebody else changed it since.
static LAST_WRITE_SEQ: AtomicU32 = AtomicU32::new(0);

/// Sequence number right after the result was parked on the clipboard for "Paste there"
/// (0 = none). Unlike `LAST_WRITE_SEQ` it is not consumed by `restore`.
static RESULT_SEQ: AtomicU32 = AtomicU32::new(0);

struct Open;

impl Open {
    fn new() -> Result<Self, String> {
        for _ in 0..10 {
            if unsafe { OpenClipboard(None) }.is_ok() {
                return Ok(Open);
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Err("OpenClipboard failed".into())
    }
}

impl Drop for Open {
    fn drop(&mut self) {
        let _ = unsafe { CloseClipboard() };
    }
}

pub fn sequence_number() -> u32 {
    unsafe { GetClipboardSequenceNumber() }
}

/// Text up to the first NUL (or the whole slice when unterminated).
fn utf16_until_nul(units: &[u16]) -> String {
    let len = units.iter().position(|&c| c == 0).unwrap_or(units.len());
    String::from_utf16_lossy(&units[..len])
}

pub fn read_text() -> Option<String> {
    let _c = Open::new().ok()?;
    unsafe {
        let h = GetClipboardData(CF_UNICODETEXT).ok()?;
        let g = HGLOBAL(h.0);
        let p = GlobalLock(g) as *const u16;
        if p.is_null() {
            return None;
        }
        // Bound the NUL scan by the allocation: a source app may place an unterminated string.
        let units = std::slice::from_raw_parts(p, GlobalSize(g) / 2);
        let s = utf16_until_nul(units);
        let _ = GlobalUnlock(g);
        Some(s)
    }
}

unsafe fn set_global(format: u32, bytes: &[u8]) -> Result<(), String> {
    let g = GlobalAlloc(GMEM_MOVEABLE, bytes.len()).map_err(|e| e.to_string())?;
    let p = GlobalLock(g) as *mut u8;
    if p.is_null() {
        let _ = GlobalFree(Some(g));
        return Err("GlobalLock failed".into());
    }
    std::ptr::copy_nonoverlapping(bytes.as_ptr(), p, bytes.len());
    let _ = GlobalUnlock(g);
    // SetClipboardData takes ownership of the handle on success.
    SetClipboardData(format, Some(HANDLE(g.0)))
        .map(|_| ())
        .map_err(|e| {
            let _ = GlobalFree(Some(g));
            e.to_string()
        })
}

fn write_text_inner(s: &str, record_seq: bool) -> Result<(), String> {
    {
        let _c = Open::new()?;
        unsafe {
            EmptyClipboard().map_err(|e| e.to_string())?;
            let bytes: Vec<u8> = s
                .encode_utf16()
                .chain(std::iter::once(0))
                .flat_map(|u| u.to_le_bytes())
                .collect();
            set_global(CF_UNICODETEXT, &bytes)?;
            let zero = 0u32.to_le_bytes();
            for name in [
                w!("ExcludeClipboardContentFromMonitorProcessing"),
                w!("CanIncludeInClipboardHistory"),
            ] {
                let fmt = RegisterClipboardFormatW(name);
                if fmt != 0 {
                    let _ = set_global(fmt, &zero);
                }
            }
        }
    }
    if record_seq {
        LAST_WRITE_SEQ.store(sequence_number(), Ordering::SeqCst);
    }
    Ok(())
}

/// Publishes text excluded from Win+V history and cloud sync.
pub fn write_text(s: &str) -> Result<(), String> {
    write_text_inner(s, true)
}

/// Like `write_text`, but remembers the clipboard version so a later "Paste there" can tell
/// whether the result is still what is on the clipboard.
pub fn write_result(s: &str) -> Result<(), String> {
    write_text_inner(s, true)?;
    RESULT_SEQ.store(sequence_number(), Ordering::SeqCst);
    Ok(())
}

/// True when nobody has touched the clipboard since `write_result`.
pub fn result_unchanged() -> bool {
    let seq = RESULT_SEQ.load(Ordering::SeqCst);
    seq != 0 && sequence_number() == seq
}

pub fn clear() -> Result<(), String> {
    LAST_WRITE_SEQ.store(0, Ordering::SeqCst);
    let _c = Open::new()?;
    unsafe { EmptyClipboard().map_err(|e| e.to_string()) }
}

struct SavedFormat {
    format: u32,
    data: Vec<u8>,
}

/// Every byte-copyable format that was on the clipboard.
pub(crate) struct Snapshot {
    formats: Vec<SavedFormat>,
}

impl Snapshot {
    pub(crate) fn empty() -> Self {
        Self {
            formats: Vec::new(),
        }
    }
}

/// Formats whose handles are GDI objects / metafiles rather than global memory: they cannot be
/// byte-copied. CF_BITMAP/CF_PALETTE are re-synthesised by Windows from CF_DIB(V5) on restore.
fn is_non_copyable(format: u32) -> bool {
    matches!(
        format,
        2 // CF_BITMAP
        | 3 // CF_METAFILEPICT
        | 9 // CF_PALETTE
        | 14 // CF_ENHMETAFILE
        | 0x80 // CF_OWNERDISPLAY
        | 0x81 // CF_DSPTEXT
        | 0x82 // CF_DSPBITMAP
        | 0x83 // CF_DSPMETAFILEPICT
        | 0x8E // CF_DSPENHMETAFILE
    ) || (0x300..=0x3FF).contains(&format) // CF_GDIOBJ range
}

/// Reads all formats. The clipboard must be open. Fails (without touching anything) when a format
/// exceeds the size limit, so we never destroy data we could not keep.
unsafe fn read_formats() -> Result<Snapshot, String> {
    let mut formats = Vec::new();
    let mut format = 0u32;
    loop {
        format = EnumClipboardFormats(format);
        if format == 0 {
            break;
        }
        if is_non_copyable(format) {
            continue;
        }
        if let Ok(handle) = GetClipboardData(format) {
            let hg = HGLOBAL(handle.0);
            let size = GlobalSize(hg);
            if size == 0 {
                continue;
            }
            if size > MAX_FORMAT_BYTES {
                tracing::warn!("clipboard format too large to snapshot ({size} bytes)");
                return Err("clipboard content too large to preserve".into());
            }
            let ptr = GlobalLock(hg) as *const u8;
            if ptr.is_null() {
                continue;
            }
            let data = std::slice::from_raw_parts(ptr, size).to_vec();
            let _ = GlobalUnlock(hg);
            formats.push(SavedFormat { format, data });
        }
    }
    Ok(Snapshot { formats })
}

pub(crate) fn snapshot_formats() -> Result<Snapshot, String> {
    let _c = Open::new()?;
    unsafe { read_formats() }
}

/// Writes the snapshot back. The clipboard must be open (and is emptied first).
pub(crate) unsafe fn write_formats(snap: &Snapshot) {
    let _ = EmptyClipboard();
    for saved in &snap.formats {
        let _ = set_global(saved.format, &saved.data);
    }
}

/// Opens the clipboard and restores the snapshot (empty snapshot => empty clipboard).
pub(crate) fn restore_formats(snap: &Snapshot) -> Result<(), String> {
    let _c = Open::new()?;
    unsafe { write_formats(snap) };
    Ok(())
}

pub struct ClipboardGuard {
    snap: Snapshot,
}

impl ClipboardGuard {
    pub fn snapshot() -> Result<Self, String> {
        Ok(Self {
            snap: snapshot_formats()?,
        })
    }

    pub fn restore(self) {
        let ours = LAST_WRITE_SEQ.swap(0, Ordering::SeqCst);
        if ours != 0 && sequence_number() != ours {
            tracing::warn!("clipboard changed by someone else; not restoring");
            return;
        }
        if let Err(e) = restore_formats(&self.snap) {
            tracing::warn!("clipboard restore failed: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn nul_scan_is_bounded() {
        let a: Vec<u16> = "привет".encode_utf16().collect();
        assert_eq!(utf16_until_nul(&a), "привет");
        let b: Vec<u16> = "ab cd".encode_utf16().collect();
        assert_eq!(utf16_until_nul(&b), "ab");
        assert_eq!(utf16_until_nul(&[]), "");
    }

    fn reg(name: &str) -> u32 {
        let wide: Vec<u16> = name.encode_utf16().chain(std::iter::once(0)).collect();
        unsafe { RegisterClipboardFormatW(windows::core::PCWSTR(wide.as_ptr())) }
    }

    fn format_bytes(format: u32) -> Option<Vec<u8>> {
        let _c = Open::new().ok()?;
        unsafe {
            let h = GetClipboardData(format).ok()?;
            let hg = HGLOBAL(h.0);
            let size = GlobalSize(hg);
            let p = GlobalLock(hg) as *const u8;
            if p.is_null() {
                return None;
            }
            let v = std::slice::from_raw_parts(p, size).to_vec();
            let _ = GlobalUnlock(hg);
            Some(v)
        }
    }

    fn dib_2x2() -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&40u32.to_le_bytes()); // biSize
        v.extend_from_slice(&2i32.to_le_bytes()); // width
        v.extend_from_slice(&2i32.to_le_bytes()); // height
        v.extend_from_slice(&1u16.to_le_bytes()); // planes
        v.extend_from_slice(&32u16.to_le_bytes()); // bpp
        v.extend_from_slice(&[0u8; 24]); // compression..clrImportant
        v.extend_from_slice(&[0x10u8; 16]); // 4 pixels
        v
    }

    #[test]
    fn non_copyable_formats_are_skipped() {
        assert!(is_non_copyable(2));
        assert!(is_non_copyable(14));
        assert!(is_non_copyable(0x305));
        assert!(!is_non_copyable(CF_UNICODETEXT));
        assert!(!is_non_copyable(8)); // CF_DIB
    }

    #[test]
    #[ignore = "uses the real Windows clipboard"]
    fn write_read_restore_roundtrip() {
        let _l = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let guard = ClipboardGuard::snapshot().unwrap();
        write_text("чистовик-тест").unwrap();
        assert_eq!(read_text().as_deref(), Some("чистовик-тест"));
        guard.restore();
    }

    #[test]
    #[ignore = "uses the real Windows clipboard"]
    fn restores_all_formats() {
        let _l = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let custom = reg("X-Test");
        let dib = dib_2x2();
        {
            let _c = Open::new().unwrap();
            unsafe {
                EmptyClipboard().unwrap();
                let a: Vec<u8> = "A"
                    .encode_utf16()
                    .chain([0])
                    .flat_map(|u| u.to_le_bytes())
                    .collect();
                set_global(CF_UNICODETEXT, &a).unwrap();
                set_global(custom, &[1, 2, 3]).unwrap();
                set_global(8, &dib).unwrap(); // CF_DIB
            }
        }
        let guard = ClipboardGuard::snapshot().unwrap();
        clear().unwrap();
        write_text("tmp").unwrap();
        assert_eq!(read_text().as_deref(), Some("tmp"));
        guard.restore();
        assert_eq!(read_text().as_deref(), Some("A"));
        assert_eq!(format_bytes(custom), Some(vec![1, 2, 3]));
        assert_eq!(format_bytes(8).map(|v| v.len()), Some(dib.len()));
        clear().unwrap();
    }

    #[test]
    #[ignore = "uses the real Windows clipboard"]
    fn restore_does_not_clobber_foreign_change() {
        let _l = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        write_text_inner("original", false).unwrap();
        let guard = ClipboardGuard::snapshot().unwrap();
        write_text("tmp").unwrap();
        write_text_inner("other", false).unwrap(); // someone else copies
        guard.restore();
        assert_eq!(read_text().as_deref(), Some("other"));
        clear().unwrap();
    }
}
