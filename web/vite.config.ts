import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";
import { staticPages } from "./build/static-pages.ts";

export default defineConfig({
  // Relative asset URLs, so the same build works at the site root and under pr/<n>/.
  base: "./",
  plugins: [react(), staticPages()],
  // Demo models are read straight from the repository (tests/, design/).
  server: { fs: { allow: [fileURLToPath(new URL("..", import.meta.url))] } },
  worker: { format: "es" },
  build: {
    chunkSizeWarningLimit: 1000,
    // The landing page, the playground and the docs.
    rolldownOptions: {
      input: {
        home: fileURLToPath(new URL("index.html", import.meta.url)),
        playground: fileURLToPath(new URL("playground.html", import.meta.url)),
        agents: fileURLToPath(new URL("docs/agents.html", import.meta.url)),
        "first-model": fileURLToPath(new URL("docs/first-model.html", import.meta.url)),
        overview: fileURLToPath(new URL("docs/overview.html", import.meta.url)),
        "coming-from-p": fileURLToPath(new URL("docs/coming-from-p.html", import.meta.url)),
        parity: fileURLToPath(new URL("docs/parity.html", import.meta.url)),
        cli: fileURLToPath(new URL("docs/cli.html", import.meta.url)),
        coverage: fileURLToPath(new URL("docs/coverage.html", import.meta.url)),
        "github-actions": fileURLToPath(new URL("docs/github-actions.html", import.meta.url)),
        python: fileURLToPath(new URL("docs/python.html", import.meta.url)),
        rust: fileURLToPath(new URL("docs/rust.html", import.meta.url)),
        go: fileURLToPath(new URL("docs/go.html", import.meta.url)),
        java: fileURLToPath(new URL("docs/java.html", import.meta.url)),
      },
    },
  },
});
