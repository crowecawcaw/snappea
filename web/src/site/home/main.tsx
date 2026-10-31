import { hydrateIslands } from "../islands";
import "../../styles.css";
import "../site.css";
import "../docs/docs.css";
import "./home.css";

hydrateIslands({
  schedules: () => import("../docs/diagrams/Schedules").then((m) => m.Schedules),
  loops: () => import("../docs/diagrams/Loops").then((m) => m.Loops),
  workflow: () => import("../docs/diagrams/Workflow").then((m) => m.Workflow),
  speed: () => import("./SpeedBars").then((m) => m.SpeedBars),
});
