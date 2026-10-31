// Check published responses: an empty client-rendered shell must fail.
import assert from "node:assert/strict";
import { readFile, readdir } from "node:fs/promises";
import { test } from "node:test";

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
      for (const island of ["schedules", "loops", "workflow"]) {
        assert.match(html, new RegExp(`data-island="${island}"`));
      }
    }
  });
}
