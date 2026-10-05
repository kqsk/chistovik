import { useState } from "react";
import {
  chordFromEvent,
  formatHotkey,
  hotkeyErrorText,
} from "../../shared/hotkey";
import type { HotkeyError } from "../../shared/ipc";
import { S } from "../../shared/strings";

/** Click, press a combo. Backspace/Delete clears, Esc leaves the field (D-13). */
export function HotkeyField({
  id,
  value,
  problem,
  label = S.editorHotkey,
  onSet,
  onChanged,
}: {
  id: string;
  value: string | null;
  /** Standing problem (e.g. key taken at startup, D-14). */
  problem: string | null;
  label?: string;
  /** Rejects with HotkeyError; the old key stays then. */
  onSet: (hotkey: string | null) => Promise<void>;
  onChanged: () => void;
}) {
  const [error, setError] = useState<string | null>(null);
  const shown = error ?? problem;
  const descId = `hk-err-${id}`;

  const apply = async (hotkey: string | null) => {
    try {
      await onSet(hotkey);
      setError(null);
    } catch (e) {
      setError(hotkeyErrorText(e as HotkeyError));
    }
    onChanged();
  };

  const onKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Escape") {
      e.currentTarget.blur();
      return;
    }
    if (e.key === "Tab") return;
    e.preventDefault();
    if (e.key === "Backspace" || e.key === "Delete") {
      void apply(null);
      return;
    }
    const chord = chordFromEvent(e.nativeEvent);
    if (chord === null) return;
    if (chord === "NO_MODIFIER") {
      setError(S.hotkeyNoModifierShort);
      return;
    }
    void apply(chord);
  };

  return (
    <div className="field">
      <input
        className="field-input hotkey-input"
        readOnly
        value={value ? formatHotkey(value) : ""}
        placeholder={S.hotkeyNone}
        aria-label={label}
        aria-invalid={shown ? "true" : undefined}
        aria-describedby={shown ? descId : undefined}
        onKeyDown={onKeyDown}
        onBlur={() => setError(null)}
      />
      {shown && (
        <span id={descId} className="field-error">
          {shown}
        </span>
      )}
    </div>
  );
}
