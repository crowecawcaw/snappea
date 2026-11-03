/** Parser-derived declarations; never inferred from event-name conventions. */
export interface SourceLocation { file: string; line: number; col: number; text: string }
export interface ModelAction {
  label: string;
  source: SourceLocation;
  targets: { state: string; conditional: boolean; source: SourceLocation }[];
}
export interface ModelState { name: string; start: boolean; hot: boolean; source: SourceLocation; actions: ModelAction[] }
export interface ModelMachine {
  name: string; kind: "machine" | "spec" | "scenario"; source: SourceLocation;
  variables: { name: string; type: string }[];
  states: ModelState[];
  rules: { condition: string; source: SourceLocation }[];
}
export interface ModelInfo {
  machines: ModelMachine[];
  tests: { name: string; main: string; composition: string; source: SourceLocation }[];
}
