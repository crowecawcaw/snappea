import { renderToString } from "react-dom/server";
import { Home } from "./home/Home";
import { DocsLayout } from "./docs/DocsLayout";
import { Cli } from "./docs/Cli";
import { Coverage } from "./docs/Coverage";
import { ComingFromP } from "./docs/ComingFromP";
import { GitHubActions } from "./docs/GitHubActions";
import { Overview } from "./docs/Overview";
import { Go } from "./docs/languages/Go";
import { Java } from "./docs/languages/Java";
import { Python } from "./docs/languages/Python";
import { Rust } from "./docs/languages/Rust";
import { Parity } from "./docs/Parity";

import { Agents } from "./docs/Agents";
import { FirstModel } from "./docs/FirstModel";

const PAGES = { agents: Agents, "first-model": FirstModel, overview: Overview, parity: Parity, "coming-from-p": ComingFromP, cli: Cli, coverage: Coverage, "github-actions": GitHubActions, python: Python, rust: Rust, go: Go, java: Java };

/** Render the same content in development and in every published HTML entry. */
export function render(page: string) {
  if (page === "home") return renderToString(<Home />);
  if (!(page in PAGES)) throw new Error(`Unknown docs page: ${page}`);
  const Page = PAGES[page as keyof typeof PAGES];
  return renderToString(<DocsLayout root="../" page={page}><Page root="../" /></DocsLayout>);
}
