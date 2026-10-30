// JavaScript wrapper for snappea_wasm.wasm (browser or Node.js, ES module).
//
//   import { loadP } from "./snappea.mjs";
//   const p = await loadP(wasmBytesOrResponse);
//   const r = p.run(["check", "model.p", "tcMain", "--timeout", "10s"], { "/work/model.p": source }, { seed: 42 });
//   console.log(r.stdout, r.exitCode);
//
// Pass `onProgress` to see a check while it runs: it is called a few times a second with
// a snapshot such as {phase: "check", test_case: "tcMain", schedules: 1240, elapsed: 3.1,
// time_limit: 10, done: false}.

export async function loadP(source) {
  const dec = new TextDecoder();
  let onProgress;
  const imports = {
    env: {
      snappea_now: () => performance.now() / 1000,
      snappea_progress: (ptr, len) => {
        if (onProgress) onProgress(JSON.parse(dec.decode(new Uint8Array(ex.memory.buffer, ptr, len))));
      },
    },
  };
  const { instance } =
    source instanceof Response || (typeof source?.then === "function")
      ? await WebAssembly.instantiateStreaming(source, imports)
      : await WebAssembly.instantiate(source, imports);
  const ex = instance.exports;
  const enc = new TextEncoder();

  function call(request) {
    const input = enc.encode(JSON.stringify(request));
    const ptr = ex.snappea_alloc(input.length);
    new Uint8Array(ex.memory.buffer, ptr, input.length).set(input);
    const out = ex.snappea_run(ptr, input.length);
    ex.snappea_free(ptr, input.length);
    const len = new DataView(ex.memory.buffer).getUint32(out, true);
    const json = dec.decode(new Uint8Array(ex.memory.buffer, out + 4, len));
    ex.snappea_result_free(out);
    return JSON.parse(json);
  }

  return {
    /** Runs `snappea <args>` over an in-memory file system ({absolutePath: text}). */
    run(args, files = {}, { cwd = "/work", seed, onProgress: progress } = {}) {
      onProgress = progress;
      try {
        return call({
          args,
          files,
          cwd,
          seed: seed ?? Math.floor(Math.random() * 2 ** 32),
          progress: !!progress,
        });
      } finally {
        onProgress = undefined;
      }
    },
  };
}
