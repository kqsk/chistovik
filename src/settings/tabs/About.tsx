import { Download, RefreshCw } from "lucide-react";
import { useEffect, useState } from "react";
import {
  onUpdate,
  openRepo,
  type UpdateStatus,
  updateCheck,
  updateInstall,
  updateStatus,
} from "../../shared/ipc";
import { S } from "../../shared/strings";

const REPO = "github.com/kqsk/chistovik";

type Note = { text: string; bad: boolean } | null;

export function AboutTab() {
  const [st, setSt] = useState<UpdateStatus | null>(null);
  const [busy, setBusy] = useState<"check" | "install" | null>(null);
  const [note, setNote] = useState<Note>(null);

  useEffect(() => {
    void updateStatus().then(setSt);
    const un = onUpdate(setSt);
    return () => {
      void un.then((f) => f());
    };
  }, []);

  const check = async () => {
    setBusy("check");
    setNote(null);
    try {
      const s = await updateCheck();
      setSt(s);
      if (!s.available) setNote({ text: S.updateNone, bad: false });
    } catch {
      setNote({ text: S.updateFailed, bad: true });
    }
    setBusy(null);
  };

  const install = async () => {
    setBusy("install");
    setNote({ text: S.updateInstalling, bad: false });
    try {
      await updateInstall();
    } catch {
      setNote({ text: S.updateInstallFailed, bad: true });
      setBusy(null);
    }
  };

  return (
    <div className="tab-body">
      <section className="card">
        <h3 className="card-title">{S.settingsTitle}</h3>
        <p className="help">{S.aboutTagline}</p>
        {st && <p>{S.aboutVersion.replace("{v}", st.current)}</p>}
        <p className="help">{S.aboutLicense}</p>
        <button
          type="button"
          className="btn-ghost link"
          title={REPO}
          onClick={() => void openRepo()}
        >
          {S.aboutRepo}
        </button>
      </section>
      {st?.available && (
        <section className="card update-banner" aria-live="polite">
          <span className="card-title">
            {S.updateAvailable.replace("{v}", st.available)}
          </span>
          <button
            type="button"
            className="btn-primary"
            disabled={busy !== null}
            onClick={() => void install()}
          >
            <Download size={16} aria-hidden="true" />
            {S.updateInstall}
          </button>
        </section>
      )}
      <div className="actions">
        <button
          type="button"
          className="btn-ghost"
          disabled={busy !== null}
          onClick={() => void check()}
        >
          <RefreshCw size={16} aria-hidden="true" />
          {busy === "check" ? S.updateChecking : S.updateCheck}
        </button>
      </div>
      {note && (
        <p className={note.bad ? "field-error" : "help"} aria-live="polite">
          {note.text}
        </p>
      )}
    </div>
  );
}
