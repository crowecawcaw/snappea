// Check published responses: an empty client-rendered shell must fail.
import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import { test } from "node:test";
import { LANGS, SECTIONS } from "../src/site/docs/languages/outline.ts";

const dist = new URL("../dist/", import.meta.url);
const pages = (await readdir(new URL("../docs/", import.meta.url)))
  .filter((file) => file.endsWith(".html"));

for (const file of ["index.html", ...pages.map((file) => `docs/${file}`)]) {
  test(`${file} contains readable HTML without executing JavaScript`, async () => {
    const html = await readFile(new URL(file, dist), "utf8");
    assert.match(html, /<main[\s>]/);
    assert.match(html, /<h1>[^<]+<\/h1>/);
    assert.match(html, /<h2[\s>]/);
    assert.match(html, /<p[\s>][\s\S]*?<\/p>/);
    assert.match(html, /<nav[\s>][\s\S]*?<a href=/);
    assert.match(html, /<pre[^>]*><code>[\s\S]*?<\/code><\/pre>/);
    assert.match(html, /<link rel="stylesheet"[^>]*href="(?:\.\.\/|\.\/)assets\//);
    if (file.startsWith("docs/")) {
      assert.match(html, /<meta name="doc:type" content="(?:tutorial|how-to|explanation|reference)"/);
      assert.match(html, /<meta name="doc:personas" content="[^"]+"/);
      assert.match(html, /<meta name="doc:questions" content="[^"]+"/);
      const id = file.slice(5, -5);
      assert.match(html, new RegExp(`href="../docs/${id}.html" aria-current="page"`));
      assert.equal((html.match(/data-island=/g) ?? []).length, id === "overview" ? 3 : id === "first-model" ? 1 : 0);
    } else {
      assert.match(html, /data-island="speed"/);
      assert.match(html, /class="mini-focus"[\s\S]*?<pre/);
      for (const island of ["mini-playground", "loops"]) {
        assert.match(html, new RegExp(`data-island="${island}"`));
      }
    }
  });
}

// Language pages share one outline so a reader switching language finds the same section.
for (const lang of LANGS) {
  test(`docs/${lang}.html has the shared section outline`, async () => {
    const html = await readFile(new URL(`docs/${lang}.html`, dist), "utf8");
    const ids = [...html.matchAll(/<h2 id="([^"]+)"/g)].map((m) => m[1]);
    assert.deepEqual(ids, [...SECTIONS], "edit languages/outline.ts and docs/language-support.md#docs-page together");
  });
}

/** Every built page, as a path relative to dist. */
async function builtPages(dir = "") {
  const out = [];
  for (const entry of await readdir(new URL(dir, dist), { withFileTypes: true })) {
    if (entry.isDirectory() && entry.name !== "assets") out.push(...(await builtPages(`${dir}${entry.name}/`)));
    else if (entry.name.endsWith(".html")) out.push(`${dir}${entry.name}`);
  }
  return out;
}

test("local links with a #fragment point at an id on the target page", async () => {
  const ids = new Map();
  const idsOf = async (file) => {
    if (!ids.has(file)) {
      const html = await readFile(new URL(file, dist), "utf8").catch(() => null);
      ids.set(file, html && new Set([...html.matchAll(/\sid="([^"]+)"/g)].map((m) => m[1])));
    }
    return ids.get(file);
  };
  const broken = [];
  for (const page of await builtPages()) {
    const html = await readFile(new URL(page, dist), "utf8");
    for (const [, href] of html.matchAll(/\shref="([^"]*#[^"]*)"/g)) {
      if (/^[a-z][a-z0-9+.-]*:/i.test(href)) continue;
      const url = new URL(href.replaceAll("&amp;", "&"), new URL(page, dist));
      let file = url.pathname.slice(dist.pathname.length);
      if (file === "" || file.endsWith("/")) file += "index.html";
      // The playground's hash is its share state, not an anchor.
      if (file === "playground.html") continue;
      const found = await idsOf(file);
      const frag = decodeURIComponent(url.hash.slice(1));
      if (!found) broken.push(`${page}: ${href}: no such page`);
      else if (!found.has(frag)) broken.push(`${page}: ${href}: no id="${frag}"`);
    }
  }
  assert.deepEqual(broken, []);
});
