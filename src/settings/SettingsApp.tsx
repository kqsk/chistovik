import { useEffect, useRef, useState } from "react";
import { onSettingsTab, providersList } from "../shared/ipc";
import { S } from "../shared/strings";
import { defaultTab, type TabId } from "./logic";
import { AboutTab } from "./tabs/About";
import { GeneralTab } from "./tabs/General";
import { HistoryTab } from "./tabs/History";
import { HotkeysTab } from "./tabs/Hotkeys";
import { PromptsTab } from "./tabs/Prompts";
import { ProvidersTab } from "./tabs/Providers";

const TABS: [TabId, string][] = [
  ["general", S.tabGeneral],
  ["prompts", S.tabPrompts],
  ["providers", S.tabProviders],
  ["hotkeys", S.tabHotkeys],
  ["history", S.tabHistory],
  ["about", S.tabAbout],
];

const isTab = (t: string | null): t is TabId => TABS.some(([id]) => id === t);

function Panel({ tab }: { tab: TabId }) {
  switch (tab) {
    case "general":
      return <GeneralTab />;
    case "prompts":
      return <PromptsTab />;
    case "providers":
      return <ProvidersTab />;
    case "history":
      return <HistoryTab />;
    case "hotkeys":
      return <HotkeysTab />;
    case "about":
      return <AboutTab />;
  }
}

export function SettingsApp() {
  const [tab, setTab] = useState<TabId | null>(null);
  const buttons = useRef<Map<TabId, HTMLButtonElement>>(new Map());

  useEffect(() => {
    const asked = new URLSearchParams(window.location.search).get("tab");
    if (isTab(asked)) setTab(asked);
    else
      void providersList()
        .then((p) => setTab(defaultTab(p)))
        .catch(() => setTab("providers"));
    // Tray «История» while the window is already open.
    const un = onSettingsTab((t) => {
      if (isTab(t)) setTab(t);
    });
    return () => {
      void un.then((f) => f());
    };
  }, []);

  if (!tab) return null;

  const onKey = (e: React.KeyboardEvent) => {
    const i = TABS.findIndex(([id]) => id === tab);
    const d = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
    if (!d) return;
    e.preventDefault();
    const next = TABS[(i + d + TABS.length) % TABS.length][0];
    setTab(next);
    buttons.current.get(next)?.focus();
  };

  return (
    <div className="shell">
      <div className="tabbar" role="tablist" onKeyDown={onKey}>
        {TABS.map(([id, label]) => (
          <button
            key={id}
            type="button"
            role="tab"
            id={`tab-${id}`}
            aria-selected={tab === id}
            aria-controls="tabpanel"
            tabIndex={tab === id ? 0 : -1}
            className={tab === id ? "tab on" : "tab"}
            ref={(el) => {
              if (el) buttons.current.set(id, el);
            }}
            onClick={() => setTab(id)}
          >
            {label}
          </button>
        ))}
      </div>
      <div
        className="tabpanel"
        id="tabpanel"
        role="tabpanel"
        aria-labelledby={`tab-${tab}`}
      >
        <Panel tab={tab} />
      </div>
    </div>
  );
}
