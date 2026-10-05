import "../shared/theme.css";
import { boot } from "../shared/boot";

void boot(() => import("./Overlay").then(({ Overlay }) => <Overlay />));
