import { hydrateIslands } from "../islands";
import "../../styles.css";
import "../site.css";
import "../docs/docs.css";
import "./home.css";

hydrateIslands({
  "mini-playground": () => import("./MiniPlayground").then((m) => m.MiniPlayground),
  loops: () => import("../docs/diagrams/Loops").then((m) => m.Loops),
  speed: () => import("./SpeedBars").then((m) => m.SpeedBars),
});
