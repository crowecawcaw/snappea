import { Logo } from "./Logo";

// Header shared by the landing page and the docs. The playground has its own.
export const REPO = "https://github.com/crowecawcaw/snappea";

export type Section = "home" | "docs" | "playground";

/** Links between pages. `root` is the relative path to the site root ("./" or "../"). */
export const links = (root: string) => ({
  home: root,
  agents: `${root}docs/agents.html`,
  firstModel: `${root}docs/first-model.html`,
  overview: `${root}docs/overview.html`,
  comingFromP: `${root}docs/coming-from-p.html`,
  parity: `${root}docs/parity.html`,
  cli: `${root}docs/cli.html`,
  coverage: `${root}docs/coverage.html`,
  githubActions: `${root}docs/github-actions.html`,
  python: `${root}docs/python.html`,
  rust: `${root}docs/rust.html`,
  go: `${root}docs/go.html`,
  java: `${root}docs/java.html`,
  playground: `${root}playground.html`,
});

export function SiteHeader({ root, current }: { root: string; current: Section }) {
  const l = links(root);
  return (
    <header className="top site-top">
      <a className="brand" href={l.home} aria-label="Snap Pea home">
        <Logo />
      </a>
      <nav className="site-nav">
        <a href={l.overview} aria-current={current === "docs" ? "page" : undefined}>Docs</a>
        <a href={l.playground} aria-current={current === "playground" ? "page" : undefined}>Playground</a>
        <a href={REPO} target="_blank" rel="noreferrer">GitHub</a>
      </nav>
    </header>
  );
}
