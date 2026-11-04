import { useEffect, useRef } from "react";
import { EditorState, type Extension } from "@codemirror/state";
import { EditorView, keymap } from "@codemirror/view";
import { basicSetup } from "codemirror";
import { indentWithTab } from "@codemirror/commands";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { linter, lintGutter, setDiagnostics, type Diagnostic as CmDiagnostic } from "@codemirror/lint";
import { tags as t } from "@lezer/highlight";
import { pLanguage } from "./pLanguage";
import type { Diagnostic } from "../protocol";

const highlight = HighlightStyle.define([
  { tag: t.keyword, color: "var(--syn-keyword)", fontWeight: "600" },
  { tag: t.typeName, color: "var(--syn-type)" },
  { tag: t.className, color: "var(--syn-event)" },
  { tag: t.special(t.variableName), color: "var(--syn-name)" },
  { tag: t.string, color: "var(--syn-string)" },
  { tag: t.number, color: "var(--syn-number)" },
  { tag: t.atom, color: "var(--syn-number)" },
  { tag: t.comment, color: "var(--syn-comment)", fontStyle: "italic" },
  { tag: t.operator, color: "var(--syn-op)" },
]);

const theme = EditorView.theme({
  "&": { height: "100%", fontSize: "var(--editor-font, 13px)", backgroundColor: "var(--editor-bg)", color: "var(--fg)" },
  ".cm-scroller": { fontFamily: "var(--mono)", lineHeight: "1.5" },
  ".cm-gutters": { backgroundColor: "var(--editor-bg)", color: "var(--muted)", border: "none" },
  ".cm-activeLine, .cm-activeLineGutter": { backgroundColor: "var(--active-line)" },
  "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection": { backgroundColor: "var(--selection) !important" },
  ".cm-cursor": { borderLeftColor: "var(--fg)" },
  ".cm-tooltip": { backgroundColor: "var(--panel)", color: "var(--fg)", border: "1px solid var(--border)" },
  ".cm-flashLine": { backgroundColor: "var(--flash)" },
  // Wrapped lines hang under the first so soft-wrapped code still reads as code.
  "&.cm-lineWrapping .cm-line": { paddingLeft: "calc(2ch + 6px)", textIndent: "-2ch" },
});

interface Props {
  path: string;
  value: string;
  diagnostics: Diagnostic[];
  /** Bump `nonce` to scroll to `line` again. */
  jump?: { line: number; nonce: number; focus?: boolean };
  onChange: (path: string, text: string) => void;
  onRun: () => void;
}

export function PEditor({ path, value, diagnostics, jump, onChange, onRun }: Props) {
  const host = useRef<HTMLDivElement>(null);
  const view = useRef<EditorView | undefined>(undefined);
  const states = useRef(new Map<string, EditorState>());
  const current = useRef(path);
  const handlers = useRef({ onChange, onRun });
  handlers.current = { onChange, onRun };

  const extensions = useRef<Extension[]>([
    basicSetup,
    keymap.of([
      { key: "Mod-Enter", run: () => (handlers.current.onRun(), true) },
      indentWithTab,
    ]),
    pLanguage,
    EditorView.lineWrapping,
    syntaxHighlighting(highlight),
    theme,
    lintGutter(),
    linter(null),
    EditorView.updateListener.of((u) => {
      if (u.docChanged) handlers.current.onChange(current.current, u.state.doc.toString());
    }),
  ]);

  const stateFor = (p: string, text: string) => {
    const s = states.current.get(p);
    if (s && s.doc.toString() === text) return s;
    return EditorState.create({ doc: text, extensions: extensions.current });
  };

  useEffect(() => {
    view.current = new EditorView({ state: stateFor(path, value), parent: host.current! });
    return () => view.current?.destroy();
  }, []);

  // Switch files (keeping each file's undo history), or take an external change.
  useEffect(() => {
    const v = view.current!;
    if (current.current !== path) {
      states.current.set(current.current, v.state);
      current.current = path;
      v.setState(stateFor(path, value));
    } else if (v.state.doc.toString() !== value) {
      v.setState(stateFor(path, value));
    }
  }, [path, value]);

  useEffect(() => {
    const v = view.current!;
    const doc = v.state.doc;
    const ds: CmDiagnostic[] = diagnostics
      .filter((d) => d.line >= 1 && d.line <= doc.lines)
      .map((d) => {
        const line = doc.line(d.line);
        const from = Math.min(line.from + Math.max(d.col - 1, 0), line.to);
        const word = /^\w+/.exec(doc.sliceString(from, line.to))?.[0].length ?? 0;
        return { from, to: Math.max(from + word, from === line.to ? from : from + 1), severity: "error", message: d.message };
      });
    v.dispatch(setDiagnostics(v.state, ds));
  }, [diagnostics, path, value]);

  useEffect(() => {
    if (!jump) return;
    const v = view.current!;
    const doc = v.state.doc;
    const line = doc.line(Math.min(Math.max(jump.line, 1), doc.lines));
    v.dispatch({ selection: { anchor: line.from }, effects: EditorView.scrollIntoView(line.from, { y: "center" }) });
    if (jump.focus !== false) v.focus();
  }, [jump]);

  return <div className="editor" ref={host} />;
}
