import { Graph, layout } from '@dagrejs/dagre';
import type { Edge, Node } from '@xyflow/react';

interface LayoutOptions<N> {
  rankdir?: 'LR' | 'TB';
  nodesep?: number;
  ranksep?: number;
  size: (n: N) => { width: number; height: number };
  /**
   * A rank longer than this (a column in LR, a row in TB) is split into parallel ones. A hub (users, referenced by most
   * tables; api/lib, imported by every module) otherwise lines all its neighbours up in one rank too long to read once
   * the graph is fitted on screen.
   */
  maxRankLength?: number;
}

/** Lays nodes out with dagre (layered, edges pointing one way) and returns them positioned for React Flow. */
export function layoutGraph<N extends Node>(nodes: N[], edges: Edge[], opts: LayoutOptions<N>): N[] {
  const g = new Graph();
  const nodesep = opts.nodesep ?? 24;
  const ranksep = opts.ranksep ?? 80;
  const dir = opts.rankdir ?? 'LR';
  g.setGraph({ rankdir: dir, nodesep, ranksep, marginx: 16, marginy: 16 });
  g.setDefaultEdgeLabel(() => ({}));
  for (const n of nodes) g.setNode(n.id, opts.size(n));
  for (const e of edges) if (e.source !== e.target) g.setEdge(e.source, e.target);
  layout(g);
  const placed = nodes.map(n => {
    const p = g.node(n.id);
    const { width, height } = opts.size(n);
    // width/height let the first fitView frame the graph before React Flow has measured the nodes.
    return { ...n, width, height, position: { x: (p?.x ?? 0) - width / 2, y: (p?.y ?? 0) - height / 2 } };
  });
  return opts.maxRankLength ? wrapRanks(placed, opts.size, opts.maxRankLength, nodesep, ranksep, dir) : placed;
}

/** Splits long ranks into parallel ones, pushing the following ranks further along the main axis. */
function wrapRanks<N extends Node>(nodes: N[], size: (n: N) => { width: number; height: number }, max: number, gap: number, rankGap: number, dir: 'LR' | 'TB'): N[] {
  const main = (n: N) => (dir === 'LR' ? n.position.x : n.position.y);
  const cross = (n: N) => (dir === 'LR' ? n.position.y : n.position.x);
  const thick = (n: N) => (dir === 'LR' ? size(n).width : size(n).height);
  const long = (n: N) => (dir === 'LR' ? size(n).height : size(n).width);
  const ranks = new Map<number, N[]>();
  for (const n of nodes) {
    const k = Math.round(main(n));
    if (!ranks.has(k)) ranks.set(k, []);
    ranks.get(k)!.push(n);
  }
  const out: N[] = [];
  let shift = 0;
  for (const k of [...ranks.keys()].sort((a, b) => a - b)) {
    const rank = ranks.get(k)!.sort((a, b) => cross(a) - cross(b));
    const thickness = Math.max(...rank.map(thick));
    let m = k + shift;
    let c = 0;
    let lanes = 1;
    for (const n of rank) {
      if (c > 0 && c + long(n) > max) { m += thickness + rankGap / 2; c = 0; lanes++; }
      out.push({ ...n, position: dir === 'LR' ? { x: m, y: c } : { x: c, y: m } });
      c += long(n) + gap;
    }
    shift += (lanes - 1) * (thickness + rankGap / 2);
  }
  return out;
}
