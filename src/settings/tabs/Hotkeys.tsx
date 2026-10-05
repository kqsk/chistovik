import { useCallback, useEffect, useState } from "react";
import { formatHotkey, hotkeyErrorText } from "../../shared/hotkey";
import {
  mainHotkeyGet,
  mainHotkeySet,
  type UndoHotkeyView,
  undoHotkeyGet,
  undoHotkeySet,
} from "../../shared/ipc";
import { S } from "../../shared/strings";
import { SavedLabel, useSavedFlash } from "../ui";
import { HotkeyField } from "./HotkeyField";

const DEFAULT_MAIN_HOTKEY = "ctrl+alt+space";

export function HotkeysTab() {
  const [undo, setUndo] = useState<UndoHotkeyView | null>(null);
  const [main, setMain] = useState<string | null>(null);
  const [saved, flash] = useSavedFlash();
  const refresh = useCallback(() => void undoHotkeyGet().then(setUndo), []);
  useEffect(refresh, [refresh]);
  useEffect(() => void mainHotkeyGet().then(setMain), []);

  // Clearing the main field (Backspace) brings back the default, it can't be empty.
  const setMainHotkey = async (hotkey: string | null) => {
    await mainHotkeySet(hotkey ?? DEFAULT_MAIN_HOTKEY);
    setMain(hotkey ?? DEFAULT_MAIN_HOTKEY);
  };

  const problem =
    undo?.error && undo.hotkey
      ? S.hotkeyDefaultFailed.replace("{key}", formatHotkey(undo.hotkey))
      : undo?.error
        ? hotkeyErrorText(undo.error)
        : null;

  return (
    <div className="tab-body">
      <div className="tab-top">
        <SavedLabel on={saved} />
      </div>
      <section className="card">
        <h3 className="card-title">{S.hotkeysPalette}</h3>
        {main && (
          <HotkeyField
            id="main"
            label={S.hotkeysPalette}
            value={main}
            problem={null}
            onSet={setMainHotkey}
            onChanged={flash}
          />
        )}
        <p className="help">{S.hotkeysPaletteHelp}</p>
      </section>
      <section className="card">
        <h3 className="card-title">{S.hotkeysUndo}</h3>
        {undo && (
          <HotkeyField
            id="undo"
            label={S.hotkeysUndo}
            value={undo.hotkey}
            problem={problem}
            onSet={undoHotkeySet}
            onChanged={() => {
              flash();
              refresh();
            }}
          />
        )}
        <p className="help">{S.hotkeysUndoHelp}</p>
      </section>
      <p className="help">{S.hotkeysCommands}</p>
    </div>
  );
}
