// Pieces every language page shares, so names and defaults change in one place.
// The page outline is in docs/language-support.md#docs-page.
import type { ReactNode } from "react";
import { Code } from "../Code";
import inventorySpec from "../../../../../examples/python/specs/inventory.p?raw";
import { LANGS } from "./outline";

export { LANGS, SECTIONS } from "./outline";

export type Lang = (typeof LANGS)[number];

/** The spec every example checks (examples/<language>/specs/inventory.p). */
export function InventorySpec() {
  return (
    <Code lang="p" title="specs/inventory.p">
      {inventorySpec}
    </Code>
  );
}

const c = (s: string) => <code>{s}</code>;

interface Setting {
  meaning: ReactNode;
  /** The setting's name in each language, or null where it does not apply. */
  name: Record<Lang, ReactNode | null>;
  default: ReactNode;
  /** A language whose default differs. */
  defaultFor?: Partial<Record<Lang, ReactNode>>;
}

const SETTINGS: Setting[] = [
  {
    meaning: "P files to load: paths, globs, or folders meaning every .p in them",
    name: { python: c("spec_files"), rust: c("spec_files"), go: c("SpecFiles"), java: c("spec_files") },
    default: "required",
  },
  {
    meaning: "Specs to check",
    name: { python: c("specs"), rust: c("specs"), go: c("Specs"), java: c("specs") },
    default: "every spec in the files",
  },
  {
    meaning: "Specs left out when every spec is checked",
    name: { python: c("exclude"), rust: c("exclude"), go: c("Exclude"), java: c("exclude") },
    default: "none",
  },
  {
    meaning: "Fail when a selected spec saw no event, which catches instrumentation that was removed",
    name: { python: c("strict"), rust: c("strict"), go: c("Strict"), java: c("strict") },
    default: "true when specs are named, else false",
  },
  {
    meaning: "Also fail when a spec ends in a hot state (still waiting for something)",
    name: { python: c("check_hot"), rust: c("check_hot"), go: c("CheckHot"), java: c("check_hot") },
    default: "false",
  },
  {
    meaning: "Event names no P file declares (usually a typo): error, warn or ignore",
    name: { python: c("unknown_events"), rust: c("unknown_events"), go: c("UnknownEvents"), java: c("unknown_events") },
    default: c("error"),
  },
  {
    meaning: "How long one check may take",
    name: { python: c("timeout"), rust: null, go: c("Timeout"), java: c("timeout") },
    default: "30 s",
  },
  {
    meaning: "Where failed checks keep their events and output",
    name: { python: c("artifacts"), rust: c("SNAPPEA_ARTIFACTS"), go: c("Artifacts"), java: c("artifacts") },
    default: "a temp folder",
  },
  {
    meaning: "Suite coverage JSON: the spec handlers, statements and branches the tests reached",
    name: { python: "env or --snappea-coverage", rust: c("coverage"), go: c("Coverage"), java: c("coverage") },
    default: "off",
  },
  {
    meaning: "Which tests are checked: auto (every test), marked, or off",
    name: { python: c("mode"), rust: c("mode"), go: c("Mode"), java: c("mode") },
    default: c("auto"),
    defaultFor: { rust: <>{c("auto")}; {c("off")} runs fixtures without checking</>, go: <>{c("auto")}; {c("off")} makes recordings inert</> },
  },
];

/** Every setting, under the language's name for it. */
export function SettingsTable({ lang }: { lang: Lang }) {
  return (
    <table className="compare ref">
      <thead>
        <tr>
          <th>Setting</th>
          <th>Effect</th>
          <th>Default</th>
        </tr>
      </thead>
      <tbody>
        {SETTINGS.filter((s) => s.name[lang] !== null).map((s, i) => (
          <tr key={i}>
            <th>{s.name[lang]}</th>
            <td>{s.meaning}</td>
            <td>{s.defaultFor?.[lang] ?? s.default}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

const ENV: { name: string; meaning: ReactNode; langs: Lang[] }[] = [
  { name: "SNAPPEA_SPEC_FILES", meaning: "Spec files, separated like PATH; overrides the configured ones", langs: ["python", "rust", "go", "java"] },
  { name: "SNAPPEA_MODE", meaning: <>{c("auto")}, {c("marked")} or {c("off")}; overrides the configured mode</>, langs: ["python", "rust", "go", "java"] },
  { name: "SNAPPEA_ARTIFACTS", meaning: "Where failed checks keep their events and output", langs: ["python", "rust", "go", "java"] },
  { name: "SNAPPEA_COVERAGE", meaning: "Suite coverage JSON path; setting it turns coverage on", langs: ["python", "rust", "go", "java"] },
  { name: "SNAPPEA_BIN", meaning: <>The {c("snappea")} binary to run, instead of the one the package finds</>, langs: ["python", "go", "java"] },
  { name: "SNAPPEA_DOWNLOAD", meaning: <>{c("0")} stops the helper downloading a checker</>, langs: ["go"] },
  { name: "SNAPPEA_EVENTS_DIR", meaning: "Set by a recording for the processes a test starts: where they write events", langs: ["python", "rust", "go", "java"] },
];

/** The environment variables the language's helper reads. */
export function EnvTable({ lang }: { lang: Lang }) {
  return (
    <table className="compare ref">
      <thead>
        <tr>
          <th>Variable</th>
          <th>Effect</th>
        </tr>
      </thead>
      <tbody>
        {ENV.filter((e) => e.langs.includes(lang)).map((e) => (
          <tr key={e.name}>
            <th>{c(e.name)}</th>
            <td>{e.meaning}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

const KINDS: { kind: string; rust: string | null; cause: ReactNode }[] = [
  { kind: "binary_missing", rust: null, cause: <>No {c("snappea")} binary; the message says how to install one</> },
  { kind: "config", rust: "Config", cause: "Bad options or config file, missing spec files, P compile errors, an unknown spec name" },
  { kind: "parser", rust: "Parser", cause: "A payload that does not match its P declaration, or an unreadable event line" },
  { kind: "validation", rust: "Validation", cause: <>{c("strict")}: a selected spec saw no relevant event</> },
  { kind: "unknown_event", rust: "UnknownEvent", cause: "An event name no P file declares" },
  { kind: "out_of_order", rust: "OutOfOrder", cause: "Live input out of order" },
  { kind: "timeout", rust: null, cause: "The check took longer than the timeout" },
];

/** SetupError kinds, as the language spells them. */
export function ErrorKinds({ lang }: { lang: Lang }) {
  const name = (k: (typeof KINDS)[number]) =>
    lang === "rust" ? k.rust && `ErrorKind::${k.rust}` : lang === "java" ? `SetupError.${k.kind.toUpperCase()}` : `"${k.kind}"`;
  return (
    <table className="compare ref">
      <thead>
        <tr>
          <th>Kind</th>
          <th>Cause</th>
        </tr>
      </thead>
      <tbody>
        {KINDS.filter((k) => name(k)).map((k) => (
          <tr key={k.kind}>
            <th>{c(name(k)!)}</th>
            <td>{k.cause}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

/** The generated API reference for each package. `root` is the site root. */
export function apiLinks(root: string): Record<Lang, { label: string; href: string }[]> {
  return {
    python: [{ label: "snappea (pdoc)", href: `${root}docs/api/python/snappea.html` }],
    rust: [
      { label: "snappea::test on docs.rs", href: "https://docs.rs/snappea/latest/snappea/test/" },
      { label: "snappea::emit on docs.rs", href: "https://docs.rs/snappea/latest/snappea/emit/" },
    ],
    go: [
      { label: "snappeatest on pkg.go.dev", href: "https://pkg.go.dev/github.com/crowecawcaw/snappea/go/snappeatest" },
      { label: "snappeaemit on pkg.go.dev", href: "https://pkg.go.dev/github.com/crowecawcaw/snappea/go/snappeaemit" },
    ],
    java: [{ label: "io.github.crowecawcaw:snappea on javadoc.io", href: "https://javadoc.io/doc/io.github.crowecawcaw/snappea" }],
  };
}

export function ApiReference({ lang, root }: { lang: Lang; root: string }) {
  return (
    <ul>
      {apiLinks(root)[lang].map((l) => (
        <li key={l.href}>
          <a href={l.href}>{l.label}</a>
        </li>
      ))}
    </ul>
  );
}

/** The quick start's buggy variant. Used inside a paragraph, so keep its HTML phrasing content. */
export function BugIntro({ children }: { children: ReactNode }) {
  return (
    <span style={{ display: "block" }}>
      Now break the service: {children} The test's own code still passes, because it accepts either outcome for the
      second order. The spec does not:
    </span>
  );
}
