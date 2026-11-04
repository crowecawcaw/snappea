// Readable, editable Playground models; upstream compatibility fixtures stay separate.
import type { Files } from "./protocol";

const sources = import.meta.glob(
  ["../demos/**/*.p", "../demos/**/*.toml"],
  { query: "?raw", import: "default", eager: true },
) as Record<string, string>;

export interface Demo {
  id: string;
  title: string;
  /** One or two sentences shown above the editor. */
  blurb: string;
  /** Source folders and where they appear in the playground's file tree. */
  roots: { from: string; to?: string }[];
  /** File opened first. */
  open: string;
  testCase: string;
  extraArgs?: string;
}

export const DEMOS: Demo[] = [
  {
    id: "lost-update",
    title: "Hello P: a lost update",
    blurb:
      "Two clients increment a shared counter with a read and then a write. Check it to find the interleaving that loses an update, then step through the trace.",
    roots: [{ from: "../demos/hello/" }],
    open: "LostUpdate.p",
    testCase: "tcTwoClients",
  },
  {
    id: "client-server",
    title: "Bank client/server",
    blurb:
      "Clients withdraw money from a bank server backed by a database. A safety spec checks balances never drop below 10; a liveness spec checks every request gets a reply. tcAbstractServer uses a deliberately buggy abstract bank.",
    roots: [{ from: `../demos/bank/` }],
    open: "PSrc/Server.p",
    testCase: "tcAbstractServer",
  },
  {
    id: "two-phase-commit",
    title: "Two-phase commit",
    blurb:
      "A coordinator runs two-phase commit over participants, with timers and an optional failure injector. The Atomicity spec checks every participant agrees on each transaction.",
    roots: [
      { from: `../demos/commit/` },
      { from: `../demos/common/Timer/`, to: "Common/Timer/" },
      { from: `../demos/common/FailureInjector/`, to: "Common/FailureInjector/" },
    ],
    open: "PSrc/Coordinator.p",
    testCase: "tcMultipleClientsWithFailure",
  },
  {
    id: "espresso",
    title: "Espresso machine",
    blurb:
      "A coffee maker and its control panel, driven by sane and 'crazy' users pressing buttons in any order. The spec checks the machine never brews in an invalid state.",
    roots: [{ from: `../demos/espresso/` }],
    open: "PSrc/CoffeeMakerControlPanel.p",
    testCase: "tcCrazyUserUsingCoffeeMachine",
  },
  {
    id: "failure-detector",
    title: "Failure detector",
    blurb:
      "A heartbeat-based failure detector with nodes that can crash. The spec checks that only crashed nodes are ever reported as failed.",
    roots: [
      { from: `../demos/detector/` },
      { from: `../demos/common/Timer/`, to: "Common/Timer/" },
      { from: `../demos/common/FailureInjector/`, to: "Common/FailureInjector/" },
    ],
    open: "PSrc/FailureDetector.p",
    testCase: "tcTest_FailureDetector",
  },
  {
    id: "paxos",
    title: "Single-decree Paxos",
    blurb: "Proposers, acceptors and learners. The spec checks at most one value is ever chosen.",
    roots: [{ from: `../demos/paxos/` }],
    open: "PSrc/proposer.p",
    testCase: "testBasicPaxos3on3",
  },
  {
    id: "raft",
    title: "Raft",
    blurb:
      "Leader election and log replication with unreliable networks, checked against safety (one leader per term, log matching, …) and liveness specs. Larger: allow more time to explore.",
    roots: [{ from: `../demos/raft/` }],
    open: "PSrc/Server.p",
    testCase: "oneClientThreeServersUnreliable",
  },
  {
    id: "retry-budget",
    title: "Time: retry budget",
    blurb:
      "Uses the snappea time extension: a client retries with timeouts and backoff, and a spec asserts every request resolves within a 1 s budget. Set timeoutAfter = ms(250) in PSrc/RetryClient.p to fix it.",
    roots: [{ from: "../demos/retry/" }],
    open: "PSrc/RetryClient.p",
    testCase: "tcRetry",
    extraArgs: "--latency PTst/latency.toml --timed",
  },
];

/** Stands in for a demo when the files were opened by the user or came from a shared link. */
export const CUSTOM: Demo = {
  id: "custom",
  title: "Your files",
  blurb:
    "Files you opened or that came from a shared link. Edits are kept in the page URL, so copy it to share this project. Drop .p files, a folder or a .zip anywhere to open others.",
  roots: [],
  open: "",
  testCase: "",
};

const ORDER = ["PSrc/", "PSpec/", "PTst/", "Common/"];

/** Sorts paths the way P projects are usually read: sources, specs, tests, shared. */
export function sortPaths(paths: string[]): string[] {
  const rank = (p: string) => {
    const i = ORDER.findIndex((o) => p.startsWith(o));
    return i < 0 ? (p.includes("/") ? ORDER.length : -1) : i;
  };
  return [...paths].sort((a, b) => rank(a) - rank(b) || a.localeCompare(b));
}

export function loadDemo(demo: Demo): Files {
  const files: Files = {};
  for (const root of demo.roots) {
    for (const [path, text] of Object.entries(sources)) {
      if (path.startsWith(root.from)) files[(root.to ?? "") + path.slice(root.from.length)] = text;
    }
  }
  return files;
}
