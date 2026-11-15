type Evidence = {
  provenance: "observed" | "checked" | "inferred"
  caption: string
  path: string | null
  text: string | null
  repro: string | null
}

type Item = {
  id: string
  kind: "finding" | "decision" | "good"
  severity: string
  section: number | null
  title: string
  body: string
  plain: string
  at: { file: string; line: number } | null
  status: "confirmed" | "unreproduced" | "resolved" | null
  evidence: Evidence[]
}

type Section = { title: string; tier: string; why: string }

type Source = { id: string; head_sha: string; url: string; reference: string; repo: string; author: string; branch: string; target: string }

type Hunk = { header: string; old_start: number; new_start: number; lines: string[] }

type Note = {
  text: string
  file: string | null
  line: number | null
  start: number | null
  old: boolean
  item: string | null
  reply_to: string | null
  posted: boolean
}

type State = {
  verdicts: Record<string, string>
  notes: Note[]
  finished: { how: string; posted: boolean } | null
  published: string[]
  quotes: Record<string, boolean>
  inherited: boolean
}

type Thread = { id: string; file: string | null; line: number | null; old: boolean; resolved: boolean; notes: [author: string, body: string, when: string][] }

type Data = {
  dir: string
  review: { title: string; summary: string; plain: string; working: string; map: Evidence | null; sources: Source[]; sections: Section[]; items: Item[] }
  files: { source: string; path: string; hunks: Hunk[] }[]
  groups: [file: number, hunk: number][][]
  state: State
  quoted: Record<string, boolean>
  links: Record<string, string>
  preview: string
  outgoing: number
  finish: string[]
  threads: Thread[]
  product: boolean
  agent: boolean
}

export type { Data, Evidence, Hunk, Item, Note, State, Thread }
