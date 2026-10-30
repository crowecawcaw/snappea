// Keeps the playground's state in the page URL, so a copied link opens the same
// project. An unedited demo is just `?demo=<id>&file=<path>`; edited or imported files
// go in the hash as deflated, base64url-encoded JSON (`#s=...`).
import { deflateSync, inflateSync, strFromU8, strToU8 } from "fflate";
import type { Files } from "./protocol";

export interface Shared {
  /** The demo the files came from, if any (keeps its title and blurb). */
  demo?: string;
  files: Files;
  open: string;
  testCase?: string;
  timeout?: number;
  extraArgs?: string;
  /** "Strict checks": fail on lint findings and coverage gaps. Absent means off. */
  strict?: boolean;
}

/** Longest encoded project put in a URL. Links much past this get cut off by chat apps and some browsers. */
export const MAX_SHARE_CHARS = 32_000;

const KEY = "s";

function toBase64Url(bytes: Uint8Array): string {
  let bin = "";
  for (let i = 0; i < bytes.length; i += 0x8000) bin += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(bin).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/, "");
}

function fromBase64Url(s: string): Uint8Array {
  const bin = atob(s.replace(/-/g, "+").replace(/_/g, "/"));
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}

export function encodeShared(s: Shared): string {
  return toBase64Url(deflateSync(strToU8(JSON.stringify(s)), { level: 9 }));
}

export function decodeShared(encoded: string): Shared {
  const s = JSON.parse(strFromU8(inflateSync(fromBase64Url(encoded))));
  if (!s || typeof s.files !== "object") throw new Error("no files");
  const files: Files = {};
  for (const [k, v] of Object.entries(s.files)) if (typeof v === "string") files[k] = v;
  return {
    demo: typeof s.demo === "string" ? s.demo : undefined,
    files,
    open: typeof s.open === "string" ? s.open : "",
    testCase: typeof s.testCase === "string" ? s.testCase : undefined,
    timeout: typeof s.timeout === "number" && Number.isFinite(s.timeout)
      ? Math.min(3600, Math.max(1, Math.trunc(s.timeout))) : undefined,
    extraArgs: typeof s.extraArgs === "string" ? s.extraArgs : undefined,
    strict: s.strict === true ? true : undefined,
  };
}

/** The encoded project in the current URL's hash, if any. */
export function sharedFromHash(hash = location.hash): string | undefined {
  return new URLSearchParams(hash.replace(/^#/, "")).get(KEY) ?? undefined;
}

/**
 * The URL for the current state. `edited` state goes in the hash; returns `tooBig`
 * when it is over MAX_SHARE_CHARS, in which case the URL carries only the demo.
 */
export function urlFor(state: Shared, edited: boolean): { url: string; tooBig: boolean } {
  const q = new URLSearchParams();
  if (state.demo) q.set("demo", state.demo);
  if (!edited) {
    if (state.open) q.set("file", state.open);
    return { url: `?${q}`, tooBig: false };
  }
  const encoded = encodeShared(state);
  const tooBig = encoded.length > MAX_SHARE_CHARS;
  const qs = q.toString();
  const search = qs ? `?${qs}` : location.pathname;
  return { url: tooBig ? search : `${search}#${KEY}=${encoded}`, tooBig };
}
