// The <h2 id> sequence every language page has, in order (docs/language-support.md#docs-page).
// Plain .ts so web/scripts/check-static-pages.mjs can import it; that check pins the built pages to it.
export const SECTIONS = [
  "install",
  "quick-start",
  "test-setup",
  "emitting-events",
  "other-processes",
  "failures",
  "reference",
] as const;

/** Each language page, as web/docs/<lang>.html. */
export const LANGS = ["python", "rust", "go", "java"] as const;
