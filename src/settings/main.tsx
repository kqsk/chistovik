import "../shared/theme.css";
import "./settings.css";
import { boot } from "../shared/boot";

// After a language switch the window reopens on «Общие», where the switch is.
void boot(
  () => import("./SettingsApp").then(({ SettingsApp }) => <SettingsApp />),
  "settings.html?tab=general",
);
