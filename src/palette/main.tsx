import "../shared/theme.css";
import { boot } from "../shared/boot";

void boot(() => import("./Palette").then(({ Palette }) => <Palette />));
