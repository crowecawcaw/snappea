// Docs shell: site header, a sidebar of pages, and the page content.
import type { ReactNode } from "react";
import { REPO, SiteHeader, links } from "../SiteHeader";

const BLOB = `${REPO}/blob/main`;

type Page = { id: string; title: string; href: (root: string) => string };

/** Docs pages on this site. Add new pages here. */
const LEARN: Page[] = [
  { id: "overview", title: "Understand models", href: (root) => links(root).overview },
  { id: "first-model", title: "Your first model", href: (root) => links(root).firstModel },
  { id: "agents", title: "Work with your agent", href: (root) => links(root).agents },
];
const CHECK: Page[] = [
  { id: "coverage", title: "Understand coverage", href: (root) => links(root).coverage },
  { id: "github-actions", title: "Run checks in CI", href: (root) => links(root).githubActions },
];
const REFERENCE: Page[] = [
  { id: "cli", title: "Command line", href: (root) => links(root).cli },
  { id: "coming-from-p", title: "Coming from P", href: (root) => links(root).comingFromP },
  { id: "parity", title: "Upstream parity", href: (root) => links(root).parity },
];
const PAGES = [...LEARN, ...CHECK, ...REFERENCE];

/** One page per language helper; each follows docs/language-support.md#docs-page. */
const LANGUAGES: Page[] = [
  { id: "python", title: "Python", href: (root) => links(root).python },
  { id: "rust", title: "Rust", href: (root) => links(root).rust },
  { id: "go", title: "Go", href: (root) => links(root).go },
  { id: "java", title: "Java", href: (root) => links(root).java },
];

/** Guides that still live in the repository. */
const GUIDES = [
  { title: "Linting models", href: `${BLOB}/docs/linting.md` },
  { title: "Checking logs (snappea observe)", href: `${BLOB}/docs/observe.md` },
  { title: "Testing and production", href: `${BLOB}/docs/production.md` },
  { title: "Rust library", href: `${BLOB}/docs/library.md` },
];

export function DocsLayout({ root, page, children }: { root: string; page: string; children: ReactNode }) {
  const title = [...PAGES, ...LANGUAGES].find((p) => p.id === page)?.title ?? "Overview";
  return (
    <div className="site docs">
      <div className="docs-header">
        <SiteHeader root={root} current="docs" />
        <details className="docs-menu">
          <summary aria-label={`Docs menu: ${title}`}>
            <span>Docs <span className="docs-menu-current">· {title}</span></span>
            <span className="docs-menu-chevron" aria-hidden="true" />
          </summary>
          <DocsIndex root={root} page={page} />
        </details>
      </div>
      <div className="docs-shell">
        <DocsIndex root={root} page={page} sidebar />
        <main className="doc">{children}</main>
      </div>
    </div>
  );
}

/** One index shared by the desktop sidebar and the native mobile menu. */
function DocsIndex({ root, page, sidebar = false }: { root: string; page: string; sidebar?: boolean }) {
  return (
    <nav className={`docs-index${sidebar ? " docs-side" : ""}`} aria-label="Docs">
      <p className="side-head">Learn and design</p>
      <PageList pages={LEARN} root={root} page={page} />
      <p className="side-head">Check your application</p>
      <PageList pages={LANGUAGES} root={root} page={page} />
      <p className="side-head">Check quality and CI</p>
      <PageList pages={CHECK} root={root} page={page} />
      <p className="side-head">Reference and migration</p>
      <PageList pages={REFERENCE} root={root} page={page} />
      <p className="side-head">More guides on GitHub</p>
      <ul>
        {GUIDES.map((g) => (
          <li key={g.title}>
            <a href={g.href} target="_blank" rel="noreferrer">{g.title} <span aria-hidden="true">↗</span></a>
          </li>
        ))}
      </ul>
    </nav>
  );
}

function PageList({ pages, root, page }: { pages: Page[]; root: string; page: string }) {
  return (
    <ul>
      {pages.map((p) => (
        <li key={p.id}>
          <a href={p.href(root)} aria-current={p.id === page ? "page" : undefined}>{p.title}</a>
        </li>
      ))}
    </ul>
  );
}
