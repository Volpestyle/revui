import { useEffect, useMemo, useState } from "react"
import type { ReactNode } from "react"
import type { Evidence } from "./types.ts"

type Node = { id: string; label: string; shape: "box" | "store" | "queue"; state?: string }
type Edge = { from: string; to: string; label: string; dashed: boolean }

const REF = /^([\w-]+)(\[\[(.+?)\]\]|\[\((.+?)\)\]|\(\((.+?)\)\)|\[(.+?)\]|\((.+?)\)|\{(.+?)\})?(?::::([\w-]+))?$/
const LINK = /\s*(?:-\.\s*([^.>]*?)\s*\.->|--\s+([^->|]+?)\s+-->|[-=]{2,}>\s*\|(.*?)\||[-=]{2,}>|-{3,})\s*/g
const SKIP = /^(flowchart|graph|classDef|subgraph|end\b|linkStyle|style|direction|%%)/
const STATES: Record<string, string> = { changed: "changed", added: "added", new: "added", removed: "removed" }

/** Reads the flowchart the guide writes for the map: nodes, who calls whom, and what the change does to each. */
function parse(text: string) {
  const nodes = new Map<string, Node>()
  const edges: Edge[] = []
  const touch = (raw: string) => {
    const found = REF.exec(raw.trim())
    if (!found) return undefined
    const [, id, shaped, queue, store, circle, box, round, diamond, state] = found
    const label = (queue ?? store ?? circle ?? box ?? round ?? diamond)?.replace(/^"|"$/g, "").replace(/<br\s*\/?>/g, " ")
    const known = nodes.get(id)
    nodes.set(id, { id, label: label ?? known?.label ?? id, shape: shaped ? (queue ? "queue" : store ? "store" : "box") : (known?.shape ?? "box"), state: STATES[state ?? ""] ?? known?.state })
    return id
  }
  for (const line of text.split("\n").map((line) => line.trim().replace(/;$/, ""))) {
    if (!line || SKIP.test(line)) continue
    const classed = /^class\s+([\w,\s-]+?)\s+([\w-]+)$/.exec(line)
    if (classed) {
      for (const id of classed[1].split(",").map((id) => id.trim())) nodes.set(id, { ...(nodes.get(id) ?? { id, label: id, shape: "box" }), state: STATES[classed[2]] })
      continue
    }
    const links = [...line.matchAll(LINK)]
    const ends = links.map((link, n) => line.slice(n ? links[n - 1].index + links[n - 1][0].length : 0, link.index))
    const groups = [...ends, line.slice(links.length ? links.at(-1)!.index + links.at(-1)![0].length : 0)].map((group) => group.split("&").flatMap((raw) => touch(raw) ?? []))
    links.forEach((link, n) => groups[n].forEach((from) => groups[n + 1].forEach((to) => edges.push({ from, to, label: (link[1] ?? link[2] ?? link[3] ?? "").trim(), dashed: link[0].includes(".") }))))
  }
  return { nodes: [...nodes.values()], edges }
}

const [A, B, RISE] = [62, 31, 18]

/** Columns by how far a component sits from the entry points, so calls read left to right; then the grid is tilted. */
function place(nodes: Node[], edges: Edge[]) {
  const depth = new Map(nodes.map((node) => [node.id, 0]))
  for (let pass = 0; pass < nodes.length; pass++) for (const edge of edges) if (edge.from !== edge.to && depth.get(edge.to)! <= depth.get(edge.from)! && depth.get(edge.from)! < nodes.length) depth.set(edge.to, depth.get(edge.from)! + 1)
  const columns = new Map<number, Node[]>()
  for (const node of nodes) columns.set(depth.get(node.id)!, [...(columns.get(depth.get(node.id)!) ?? []), node])
  const tallest = Math.max(...[...columns.values()].map((column) => column.length))
  const at = new Map<string, { x: number; y: number; back: number }>()
  for (const [column, members] of columns) {
    members.forEach((node, n) => {
      const row = n + (tallest - members.length) / 2
      at.set(node.id, { x: (column - row) * 128, y: (column + row) * 66, back: column + row })
    })
  }
  return at
}

function Block({ node, x, y }: { node: Node; x: number; y: number }) {
  const rise = node.shape === "store" ? 30 : RISE
  const top = y - rise
  const faces =
    node.shape === "store" ? (
      <>
        <path className="side" d={`M${x - 46} ${top}V${y}A46 23 0 0 0 ${x + 46} ${y}V${top}`} />
        <ellipse className="top" cx={x} cy={top} rx={46} ry={23} />
      </>
    ) : (
      <>
        <path className="side left" d={`M${x - A} ${top}L${x} ${top + B}V${y + B}L${x - A} ${y}Z`} />
        <path className="side" d={`M${x} ${top + B}L${x + A} ${top}V${y}L${x} ${y + B}Z`} />
        <path className="top" d={`M${x} ${top - B}L${x + A} ${top}L${x} ${top + B}L${x - A} ${top}Z`} />
        {node.shape === "queue" && <path className="seam" d={`M${x - A} ${top + rise / 2}L${x} ${top + B + rise / 2}L${x + A} ${top + rise / 2}`} />}
      </>
    )
  return <g data-state={node.state}>{faces}</g>
}

const riseOf = (node: Node) => (node.shape === "store" ? 30 : RISE)

type Shown = { caption: string; text?: string | null; path?: string | null }

/** The map as tilted blocks, with what is new, changed or removed lit. Drawn from a review's own map (its flowchart sits beside the picture), or from the repo's system map with the components this change touches. Falls back when there is no flowchart to read. */
function SystemMap({ evidence, system, fallback }: { evidence?: Evidence; system?: { text: string; touched: Record<string, number> }; fallback: ReactNode }) {
  const [fetched, setFetched] = useState<string>()
  const source = system ? undefined : evidence?.path?.replace(/\.[a-z]+$/, ".mmd")
  useEffect(() => {
    if (source) fetch(`/bundle/${source}`).then((got) => (got.ok ? got.text() : ""), () => "").then(setFetched)
  }, [source])
  const text = system?.text ?? fetched
  const drawn = useMemo(() => {
    const { nodes, edges } = parse(text ?? "")
    const lit = nodes.map((node) => (system ? { ...node, state: system.touched[node.id] ? "changed" : undefined } : node))
    return nodes.length < 2 ? undefined : { nodes: lit, edges, at: place(nodes, edges) }
  }, [text, system])
  if ((!source && !system) || text === "" || (text !== undefined && !drawn)) return fallback
  if (!drawn) return null
  const hit = drawn.nodes.filter((node) => node.state)
  const shown: Shown = evidence ?? { caption: hit.length ? `Touches ${hit.length} of ${drawn.nodes.length} parts of the system: ${hit.map((node) => node.label).join(", ")}` : "None of this change's files sit in a mapped part of the system" }
  const spots = [...drawn.at.values()]
  const [left, right, up, down] = [Math.min(...spots.map((s) => s.x)) - 110, Math.max(...spots.map((s) => s.x)) + 110, Math.min(...spots.map((s) => s.y)) - 96, Math.max(...spots.map((s) => s.y)) + 54]
  const states = [...new Set(drawn.nodes.flatMap((node) => node.state ?? []))]
  return (
    <figure className="iso">
      <svg viewBox={`${left} ${up} ${right - left} ${down - up}`} role="img" aria-label={shown.caption} style={{ maxHeight: Math.max(240, down - up) }}>
        <defs>
          <marker id="iso-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M0 0L10 5L0 10z" /></marker>
        </defs>
        {[...drawn.nodes].sort((a, b) => drawn.at.get(a.id)!.back - drawn.at.get(b.id)!.back).map((node) => <Block key={node.id} node={node} {...drawn.at.get(node.id)!} />)}
        {drawn.edges.map((edge, n) => {
          const top = (id: string) => ({ x: drawn.at.get(id)!.x, y: drawn.at.get(id)!.y - riseOf(drawn.nodes.find((node) => node.id === id)!) })
          const [from, to] = [top(edge.from), top(edge.to)]
          const [dx, dy] = [to.x - from.x, to.y - from.y]
          const reach = Math.min(0.45, 1.06 / (Math.abs(dx) / A + Math.abs(dy) / B || 1))
          return (
            <g key={n} className="call" data-new={edge.dashed || undefined}>
              <path d={`M${from.x + dx * reach} ${from.y + dy * reach}L${to.x - dx * reach} ${to.y - dy * reach}`} markerEnd="url(#iso-arrow)" />
              {edge.label && <text x={from.x + dx / 2} y={from.y + dy / 2 - 6}>{edge.label}</text>}
            </g>
          )
        })}
        {drawn.nodes.map((node) => <text key={node.id} x={drawn.at.get(node.id)!.x} y={drawn.at.get(node.id)!.y - riseOf(node) - B - 9}>{node.label}</text>)}
      </svg>
      <figcaption>
        <strong>{shown.caption}</strong>
        <span className="legend">
          {states.map((state) => <span key={state} data-state={state}>{system ? "touched" : state}</span>)}
          {shown.path && <a href={`/bundle/${shown.path}`} target="_blank" rel="noreferrer">flat diagram</a>}
        </span>
        {shown.text && <p>{shown.text}</p>}
      </figcaption>
    </figure>
  )
}

export { parse, SystemMap }
