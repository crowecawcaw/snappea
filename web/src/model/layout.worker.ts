/// <reference lib="webworker" />
import ELK from "elkjs/lib/elk.bundled.js";
import type { ElkNode } from "elkjs/lib/elk-api";
const elk = new ELK();
self.onmessage = async (event: MessageEvent<{ graph: ElkNode }>) => {
  try { self.postMessage({ graph: await elk.layout(event.data.graph) }); }
  catch (error) { self.postMessage({ error: String(error) }); }
};
