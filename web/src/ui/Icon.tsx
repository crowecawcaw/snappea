export type IconName = "actor" | "message" | "rule" | "failure" | "code" | "model" | "replay" | "console" | "check";
const paths: Record<IconName, string> = {
  actor: "M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2 M13 7a4 4 0 1 1-8 0 4 4 0 0 1 8 0",
  message: "M3 5h18v14H3z M3 5l9 7 9-7",
  rule: "M12 3l8 3v6c0 5-8 9-8 9s-8-4-8-9V6z M8 12l3 3 5-6",
  failure: "M12 3L2 21h20z M12 9v5 M12 17v1",
  code: "M8 6l-6 6 6 6 M16 6l6 6-6 6 M14 3l-4 18",
  model: "M3 3h6v6H3z M15 15h6v6h-6z M6 9v9h9 M9 6h9v9",
  replay: "M5 4l15 8-15 8z",
  console: "M4 6l6 6-6 6 M13 18h7",
  check: "M4 12l5 5L20 6",
};
export function Icon({ name }: { name: IconName }) {
  return <svg className="ui-icon" width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d={paths[name]} /></svg>;
}
