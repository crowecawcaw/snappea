// Node smoke test: node smoke.mjs path/to/snappea_wasm.wasm
import { readFile } from "node:fs/promises";
import { loadP } from "./snappea.mjs";

const p = await loadP(await readFile(process.argv[2]));
const model = `
event Ping; event Pong;
machine Main {
  var other: machine;
  start state Init { entry { other = new Echo(this); send other, Ping; } on Pong goto Done; }
  state Done { entry { assert false, "reached done"; } }
}
machine Echo { var m: machine; start state S { entry (x: machine) { m = x; } on Ping do { send m, Pong; } } }
`;
const r = p.run(["check", "pingpong.p", "--timeout", "1s"], { "/work/pingpong.p": model }, { seed: 1 });
process.stdout.write(r.stdout);
const trace = r.files["/work/PCheckerOutput/DefaultImpl/bug.txt"] ?? "";
if (r.exitCode !== 1 || !trace.includes("<ErrorLog> Assertion Failed")) throw new Error("expected a bug");
// A finite safe model should finish its exhaustive search before the deadline.
const safe = p.run(["check", "safe.p", "--timeout", "1s"], {
  "/work/safe.p": "// snappea-lint: allow no-model-machine -- single-machine fixture tests exhaustive search completion\nmachine Main { start state Init { } }",
}, { seed: 1 });
if (safe.exitCode !== 0 || !/\(all \d+ schedules?/.test(safe.stdout)) {
  throw new Error(`expected exhaustive completion: ${safe.stdout}${safe.stderr}`);
}
// Timed models use sampling, so a safe run must stop at the deadline as incomplete.
const started = performance.now();
const busy = p.run(["check", "busy.p", "--timed", "--timeout", "1s", "--json-report", "busy.json"], {
  "/work/busy.p": "// snappea-lint: allow no-model-machine -- single-machine fixture tests the timed search deadline\nmachine Main { start state Init { } }",
}, { seed: 1 });
const busyReport = JSON.parse(busy.files["/work/busy.json"]);
if (busy.exitCode !== 0 || busyReport.summary.status !== "search_incomplete"
    || busyReport.summary.incomplete !== 1 || !busy.stdout.includes(busyReport.summary.message)
    || performance.now() - started > 5000) {
  throw new Error(`expected bounded incomplete search: ${busy.stdout}${busy.stderr}`);
}
console.log("smoke test OK");
