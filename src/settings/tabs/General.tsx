import { useEffect, useState } from "react";
import {
  autostartGet,
  autostartSet,
  type BusyPosition,
  busyPositionGet,
  busyPositionSet,
  type FocusLoss,
  type GeneralSettings,
  generalGet,
  generalSet,
  type LangSetting,
  languageGet,
  languageSet,
  type PalettePosition,
  type ResultMode,
} from "../../shared/ipc";
import { S } from "../../shared/strings";
import { SavedLabel, Segmented, useSavedFlash } from "../ui";

const MODES: [ResultMode, string][] = [
  ["immediate", S.modeImmediate],
  ["preview", S.modePreview],
  ["combo", S.modeCombo],
];
const MODE_HELP: Record<ResultMode, string> = {
  immediate: S.modeImmediateHelp,
  preview: S.modePreviewHelp,
  combo: S.modeComboHelp,
};
const POSITIONS: [PalettePosition, string][] = [
  ["cursor", S.posCursor],
  ["caret", S.posCaret],
  ["center", S.posCenter],
];
const FOCUS: [FocusLoss, string][] = [
  ["close", S.focusClose],
  ["stay", S.focusStay],
];
const LANGS: [LangSetting, string][] = [
  ["auto", S.langAuto],
  ["ru", S.langRu],
  ["en", S.langEn],
];
const BUSY: [BusyPosition, string][] = [
  ["cursor", S.busyPosCursor],
  ["center", S.busyPosCenter],
  ["follow", S.busyPosFollow],
];

export function GeneralTab() {
  const [g, setG] = useState<GeneralSettings | null>(null);
  const [busy, setBusy] = useState<BusyPosition>("cursor");
  const [autostart, setAutostart] = useState(false);
  const [lang, setLangSetting] = useState<LangSetting>("auto");
  const [saved, flash] = useSavedFlash();

  useEffect(() => {
    void generalGet().then(setG);
    void busyPositionGet().then(setBusy);
    void autostartGet().then(setAutostart);
    void languageGet().then((v) => setLangSetting(v.setting));
  }, []);

  if (!g) return null;
  const change = (patch: Partial<GeneralSettings>) => {
    const next = { ...g, ...patch };
    setG(next);
    void generalSet(next).then(flash);
  };

  return (
    <div className="tab-body">
      <div className="tab-top">
        <SavedLabel on={saved} />
      </div>
      <section className="card">
        <h3 className="card-title">{S.language}</h3>
        <Segmented
          label={S.language}
          value={lang}
          options={LANGS}
          onChange={(v) => {
            setLangSetting(v);
            // The window reloads in the new language (app://language).
            void languageSet(v);
          }}
        />
      </section>
      <section className="card">
        <h3 className="card-title">{S.autostart}</h3>
        <label className="toggle">
          <input
            type="checkbox"
            checked={autostart}
            onChange={(e) => {
              const on = e.target.checked;
              setAutostart(on);
              void autostartSet(on)
                .then(flash)
                .catch(() => setAutostart(!on));
            }}
          />
          {S.autostartLabel}
        </label>
        <p className="help">{S.autostartHelp}</p>
      </section>
      <section className="card">
        <h3 className="card-title">{S.resultMode}</h3>
        <Segmented
          label={S.resultMode}
          value={g.result_mode}
          options={MODES}
          onChange={(v) => change({ result_mode: v })}
        />
        <p className="help">{MODE_HELP[g.result_mode]}</p>
      </section>
      <section className="card">
        <h3 className="card-title">{S.palettePosition}</h3>
        <Segmented
          label={S.palettePosition}
          value={g.palette_position}
          options={POSITIONS}
          onChange={(v) => change({ palette_position: v })}
        />
        {g.palette_position === "caret" && (
          <p className="help">{S.posCaretHelp}</p>
        )}
      </section>
      <section className="card">
        <h3 className="card-title">{S.focusLoss}</h3>
        <Segmented
          label={S.focusLoss}
          value={g.focus_loss}
          options={FOCUS}
          onChange={(v) => change({ focus_loss: v })}
        />
      </section>
      <section className="card">
        <h3 className="card-title">{S.previewEditable}</h3>
        <label className="toggle">
          <input
            type="checkbox"
            checked={g.preview_editable}
            onChange={(e) => change({ preview_editable: e.target.checked })}
          />
          {S.previewEditableLabel}
        </label>
        <p className="help">{S.previewEditableHelp}</p>
      </section>
      <section className="card">
        <h3 className="card-title">{S.busyPosition}</h3>
        <Segmented
          label={S.busyPosition}
          value={busy}
          options={BUSY}
          onChange={(v) => {
            setBusy(v);
            void busyPositionSet(v).then(flash);
          }}
        />
      </section>
    </div>
  );
}
