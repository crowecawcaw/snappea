import { useEffect, useRef, useState } from "react";

/** One block of console output: what a command printed, or a note. */
export interface ConsoleEntry {
  id: number;
  text: string;
}

const LOC_G = /([\w./-]+\.p):(\d+)(?::(\d+))?/g;

export const CONSOLE_HELP = `Type a snappea command and press Enter. The project's files are in the working folder.
  tests                      list test cases
  check tcMain               check one test case
  check tcMain --seed 7      check with a fixed seed
  replay PCheckerOutput/tcMain/bug.schedule
                             re-run the last bug found
  --help                     every command and option
Also: ls, cat <file>, clear. ↑/↓ recall earlier commands.`;

/**
 * The checker's output, with an input line for commands. Output lines that name a
 * source location (`a.p:12:5`) open it in the editor.
 */
export function Console({ entries, busy, onCommand, onStop, onOpenLocation }: {
  entries: ConsoleEntry[];
  busy: boolean;
  onCommand: (line: string) => void;
  onStop: () => void;
  onOpenLocation: (file: string, line: number) => void;
}) {
  const [input, setInput] = useState("");
  const history = useRef<string[]>([]);
  const [back, setBack] = useState(-1);
  const scroller = useRef<HTMLDivElement>(null);
  const field = useRef<HTMLInputElement>(null);

  // Follow new output.
  useEffect(() => {
    const el = scroller.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [entries, busy]);

  const submit = () => {
    const line = input.trim();
    setInput("");
    setBack(-1);
    if (!line) return;
    if (history.current[history.current.length - 1] !== line) history.current.push(line);
    onCommand(line);
  };

  const recall = (step: number) => {
    const h = history.current;
    if (!h.length) return;
    const next = back === -1 ? (step < 0 ? h.length - 1 : -1) : back + step;
    if (next >= h.length || next < 0) {
      setBack(-1);
      setInput("");
      return;
    }
    setBack(next);
    setInput(h[next]);
  };

  return (
    <div
      className="console-pane"
      onClick={() => {
        // Clicking the output focuses the prompt, unless the user is selecting text.
        if (!getSelection()?.toString()) field.current?.focus({ preventScroll: true });
      }}
    >
      <div className="console" ref={scroller} role="log" aria-label="Console output">
        {entries.length === 0 && <div className="line cmd">Run Check, or type a command below. Type help for examples.</div>}
        {entries.map((e) => (
          <div key={e.id} className="console-entry">
            {e.text.split("\n").map((line, i) => (
              <ConsoleLine key={i} line={line} onOpenLocation={onOpenLocation} />
            ))}
          </div>
        ))}
        {busy && <div className="line cmd">Running…</div>}
      </div>
      <form
        className="console-input"
        onSubmit={(e) => {
          e.preventDefault();
          if (!busy) submit();
        }}
      >
        <span className="prompt" aria-hidden="true">$ snappea</span>
        <input
          ref={field}
          value={input}
          onChange={(e) => setInput(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "ArrowUp") {
              e.preventDefault();
              recall(-1);
            } else if (e.key === "ArrowDown") {
              e.preventDefault();
              recall(1);
            } else if (e.key === "c" && e.ctrlKey && busy) {
              e.preventDefault();
              onStop();
            }
          }}
          placeholder={busy ? "Running… (Ctrl+C to stop)" : "check tcMain --seed 7"}
          aria-label="Console command"
          spellCheck={false}
          autoCapitalize="off"
          autoCorrect="off"
          autoComplete="off"
          enterKeyHint="go"
        />
        {busy && (
          <button type="button" className="ghost stop" onClick={onStop}>
            Stop
          </button>
        )}
      </form>
    </div>
  );
}

function ConsoleLine({ line, onOpenLocation }: { line: string; onOpenLocation: (f: string, l: number) => void }) {
  const cls =
    /^FAIL\s|\bfailed in\b|^error\b|Error|Assertion Failed|\bbug found\b|found a bug|Found [1-9]/i.test(line)
      ? "line bad"
      : /^ok\s|\bpassed in\b|Found 0 bug/.test(line)
        ? "line good"
        : line.startsWith("$ ")
          ? "line cmd"
          : "line";
  const parts: React.ReactNode[] = [];
  let last = 0;
  for (const m of line.matchAll(LOC_G)) {
    parts.push(line.slice(last, m.index));
    const [all, file, ln] = m;
    parts.push(
      <button key={m.index} className="loc" onClick={() => onOpenLocation(file, Number(ln))}>
        {all}
      </button>,
    );
    last = (m.index ?? 0) + all.length;
  }
  parts.push(line.slice(last));
  return <div className={cls}>{parts}</div>;
}

/** Splits a command line into words, honoring "double" and 'single' quotes. */
export function splitCommand(line: string): string[] {
  const out: string[] = [];
  for (const m of line.matchAll(/"([^"]*)"|'([^']*)'|(\S+)/g)) out.push(m[1] ?? m[2] ?? m[3]);
  return out;
}
