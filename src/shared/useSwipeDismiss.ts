import { invoke } from "@tauri-apps/api/core";
import { useEffect, useRef } from "react";

/** Released past this many px (or flicked) the window closes; otherwise it snaps back. */
const DISTANCE = 90;
const FLICK = 0.6; // px per ms
/** Touchpad two-finger swipe: wheel deltaX summed until it pauses this long. */
const WHEEL_IDLE = 160;
/** A drag never starts on these: text fields, buttons, selectable text. */
const NO_SWIPE = "input, textarea, button, select, .preview-text";

const nudge = (dx: number, end = false) =>
  invoke<void>("window_nudge", { dx, end }).catch(() => {});

/**
 * Swipe right to close (TODO 6): the whole window follows the pointer (mouse, touch, pen) or a
 * touchpad two-finger swipe, then closes or snaps back. Returns a ref for the root element.
 */
export function useSwipeDismiss<T extends HTMLElement>(
  onDismiss: () => void,
  enabled = true,
  /** Re-attach when the root element may have been replaced (e.g. palette list vs preview). */
  element: unknown = null,
) {
  const ref = useRef<T>(null);
  const dismissRef = useRef(onDismiss);
  dismissRef.current = onDismiss;

  // biome-ignore lint/correctness/useExhaustiveDependencies: `element` only triggers re-attach
  useEffect(() => {
    const el = ref.current;
    if (!el || !enabled) return;
    let startX = 0;
    let startY = 0;
    let dx = 0;
    let tracking = false;
    let dragging = false;
    let suppressClick = false;
    let lastX = 0;
    let lastT = 0;
    let velocity = 0;
    let frame = 0;
    let wheelTimer = 0;

    const move = (x: number) => {
      dx = Math.max(0, x);
      el.style.opacity = String(Math.max(0.35, 1 - dx / 300));
      if (!frame) {
        frame = requestAnimationFrame(() => {
          frame = 0;
          void nudge(dx);
        });
      }
    };

    const finish = (close: boolean) => {
      cancelAnimationFrame(frame);
      frame = 0;
      if (close) {
        void nudge(dx, true);
        dismissRef.current();
        // The window is hidden by now; the next show starts fully opaque.
        window.setTimeout(() => {
          el.style.opacity = "";
        }, 250);
      } else {
        void nudge(0, true);
        el.style.opacity = "";
      }
      dx = 0;
    };

    const onDown = (e: PointerEvent) => {
      if (e.button !== 0 || (e.target as Element).closest(NO_SWIPE)) return;
      tracking = true;
      dragging = false;
      startX = e.screenX;
      startY = e.screenY;
      lastX = e.screenX;
      lastT = e.timeStamp;
      velocity = 0;
    };

    const onMove = (e: PointerEvent) => {
      if (!tracking) return;
      const x = e.screenX - startX;
      const y = e.screenY - startY;
      if (!dragging) {
        // Mostly vertical, or leftwards: a click or a list scroll, not a swipe.
        if (Math.abs(y) > 8 && Math.abs(y) > Math.abs(x)) tracking = false;
        if (x < 6) return;
        dragging = true;
        el.setPointerCapture(e.pointerId);
      }
      const dt = e.timeStamp - lastT;
      if (dt > 0) velocity = (e.screenX - lastX) / dt;
      lastX = e.screenX;
      lastT = e.timeStamp;
      move(x);
    };

    const onUp = (e: PointerEvent) => {
      if (!tracking) return;
      tracking = false;
      if (!dragging) return;
      dragging = false;
      suppressClick = true;
      if (el.hasPointerCapture(e.pointerId))
        el.releasePointerCapture(e.pointerId);
      finish(dx > DISTANCE || velocity > FLICK);
    };

    // A drag that started on a row must not also run it.
    const onClick = (e: MouseEvent) => {
      if (!suppressClick) return;
      suppressClick = false;
      e.preventDefault();
      e.stopPropagation();
    };

    // Windows touchpads with the default scroll direction report fingers moving right as
    // negative deltaX. Only clearly horizontal gestures count.
    const onWheel = (e: WheelEvent) => {
      if (Math.abs(e.deltaX) <= Math.abs(e.deltaY)) return;
      move(dx - e.deltaX);
      window.clearTimeout(wheelTimer);
      wheelTimer = window.setTimeout(() => finish(dx > DISTANCE), WHEEL_IDLE);
    };

    el.addEventListener("pointerdown", onDown);
    el.addEventListener("pointermove", onMove);
    el.addEventListener("pointerup", onUp);
    el.addEventListener("pointercancel", onUp);
    el.addEventListener("click", onClick, true);
    el.addEventListener("wheel", onWheel, { passive: true });
    return () => {
      el.removeEventListener("pointerdown", onDown);
      el.removeEventListener("pointermove", onMove);
      el.removeEventListener("pointerup", onUp);
      el.removeEventListener("pointercancel", onUp);
      el.removeEventListener("click", onClick, true);
      el.removeEventListener("wheel", onWheel);
      window.clearTimeout(wheelTimer);
      cancelAnimationFrame(frame);
    };
  }, [enabled, element]);

  return ref;
}
