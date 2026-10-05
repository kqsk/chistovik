//! Where the palette goes (PAL-05). Pure integer math in physical pixels of the virtual
//! desktop, so it is testable without a window or a monitor.

use serde::{Deserialize, Serialize};

/// Setting `palette_position`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PalettePosition {
    /// Next to the mouse cursor.
    #[default]
    Cursor,
    /// Under the text caret; falls back to the cursor when the app does not tell.
    Caret,
    /// Centre of the work area of the monitor under the cursor.
    Center,
}

/// left, top, right, bottom (right/bottom exclusive).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn new(left: i32, top: i32, right: i32, bottom: i32) -> Self {
        Self {
            left,
            top,
            right,
            bottom,
        }
    }
}

/// What the palette is placed against.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    Point(i32, i32),
    /// Caret rectangle: the palette goes under it, or above when there is no room.
    Caret(Rect),
    Center,
}

const OFFSET: i32 = 8;

/// Clamps `size` so the window never exceeds the work area.
pub fn fit_size(size: (i32, i32), work: Rect) -> (i32, i32) {
    (
        size.0.min(work.right - work.left).max(1),
        size.1.min(work.bottom - work.top).max(1),
    )
}

/// Top-left of a window of `size` (already fitted) so that it stays inside `work`.
pub fn place(anchor: Anchor, size: (i32, i32), work: Rect) -> (i32, i32) {
    let (w, h) = size;
    let (mut x, mut y) = match anchor {
        Anchor::Point(px, py) => {
            let x = if px + OFFSET + w > work.right {
                px - OFFSET - w
            } else {
                px + OFFSET
            };
            let y = if py + OFFSET + h > work.bottom {
                py - OFFSET - h
            } else {
                py + OFFSET
            };
            (x, y)
        }
        Anchor::Caret(r) => {
            let y = if r.bottom + OFFSET + h > work.bottom {
                r.top - OFFSET - h
            } else {
                r.bottom + OFFSET
            };
            (r.left, y)
        }
        Anchor::Center => (
            work.left + (work.right - work.left - w) / 2,
            work.top + (work.bottom - work.top - h) / 2,
        ),
    };
    x = x.clamp(work.left, (work.right - w).max(work.left));
    y = y.clamp(work.top, (work.bottom - h).max(work.top));
    (x, y)
}

/// Physical window height after a preview grows: keeps the top, moves up only if the bottom
/// would leave the work area. Returns the new top.
pub fn keep_bottom_inside(top: i32, height: i32, work: Rect) -> i32 {
    if top + height > work.bottom {
        (work.bottom - height).max(work.top)
    } else {
        top
    }
}

/// Logical -> physical size for a monitor scale (e.g. 1.5 for 150 %).
pub fn to_physical(logical: (f64, f64), scale: f64) -> (i32, i32) {
    (
        (logical.0 * scale).round() as i32,
        (logical.1 * scale).round() as i32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const WORK: Rect = Rect {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1040,
    };

    #[test]
    fn cursor_goes_right_and_below() {
        assert_eq!(place(Anchor::Point(100, 100), (440, 300), WORK), (108, 108));
    }

    #[test]
    fn cursor_near_right_edge_flips_left() {
        let (x, _) = place(Anchor::Point(1900, 100), (440, 300), WORK);
        assert_eq!(x, 1900 - 8 - 440);
    }

    #[test]
    fn cursor_near_bottom_flips_up() {
        let (_, y) = place(Anchor::Point(100, 1030), (440, 300), WORK);
        assert_eq!(y, 1030 - 8 - 300);
    }

    #[test]
    fn corner_stays_inside() {
        let (x, y) = place(Anchor::Point(1919, 1039), (440, 300), WORK);
        assert!(x >= 0 && x + 440 <= 1920);
        assert!(y >= 0 && y + 300 <= 1040);
    }

    #[test]
    fn negative_coordinates_second_monitor_on_the_left() {
        let work = Rect::new(-1920, 0, 0, 1040);
        let (x, y) = place(Anchor::Point(-10, 500), (440, 300), work);
        assert_eq!(x, -10 - 8 - 440);
        assert_eq!(y, 508);
        let (x, _) = place(Anchor::Point(-1919, 500), (440, 300), work);
        assert_eq!(x, -1919 + 8);
        let (x, y) = place(Anchor::Center, (440, 300), work);
        assert_eq!((x, y), (-1920 + 740, 370));
    }

    #[test]
    fn taskbar_on_top_respects_work_area() {
        let work = Rect::new(0, 48, 1920, 1080);
        let (_, y) = place(Anchor::Point(500, 10), (440, 300), work);
        assert_eq!(y, 48);
    }

    #[test]
    fn caret_goes_under_the_line() {
        let r = Rect::new(300, 200, 302, 220);
        assert_eq!(place(Anchor::Caret(r), (440, 300), WORK), (300, 228));
    }

    #[test]
    fn caret_near_bottom_flips_above_the_line() {
        let r = Rect::new(300, 900, 302, 920);
        let (x, y) = place(Anchor::Caret(r), (440, 300), WORK);
        assert_eq!(x, 300);
        assert_eq!(y, 900 - 8 - 300);
    }

    #[test]
    fn caret_near_right_edge_is_pulled_in() {
        let r = Rect::new(1900, 200, 1902, 220);
        let (x, _) = place(Anchor::Caret(r), (440, 300), WORK);
        assert_eq!(x, 1920 - 440);
    }

    #[test]
    fn center_is_centered() {
        assert_eq!(place(Anchor::Center, (440, 300), WORK), (740, 370));
    }

    #[test]
    fn window_taller_than_work_area_is_fitted() {
        let size = fit_size((440, 2000), WORK);
        assert_eq!(size, (440, 1040));
        assert_eq!(place(Anchor::Point(10, 10), size, WORK).1, 0);
    }

    #[test]
    fn scale_converts_to_physical() {
        assert_eq!(to_physical((440.0, 140.0), 1.5), (660, 210));
        assert_eq!(to_physical((440.0, 140.0), 1.0), (440, 140));
        assert_eq!(to_physical((440.0, 140.0), 1.25), (550, 175));
    }

    #[test]
    fn growing_preview_moves_up_only_when_needed() {
        assert_eq!(keep_bottom_inside(100, 300, WORK), 100);
        assert_eq!(keep_bottom_inside(900, 300, WORK), 740);
        assert_eq!(keep_bottom_inside(900, 5000, WORK), 0);
    }

    #[test]
    fn position_setting_roundtrips() {
        assert_eq!(
            serde_json::from_str::<PalettePosition>("\"caret\"").unwrap(),
            PalettePosition::Caret
        );
        assert_eq!(PalettePosition::default(), PalettePosition::Cursor);
    }
}
