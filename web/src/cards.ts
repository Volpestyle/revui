import type { Data, Item, Thread } from "./types.ts"

type Card =
  | { key: string; label: string; kind: "summary" }
  | { key: string; label: string; kind: "part"; group: number }
  | { key: string; label: string; kind: "item"; item: Item }
  | { key: string; label: string; kind: "thread"; thread: Thread }
  | { key: string; label: string; kind: "note"; note: number }

const KINDS = { finding: "finding", decision: "decision", good: "good call" }

function place(file: string | null, line: number | null) {
  return file && line ? `${file}:${line}` : "the whole merge request"
}

function cardsOf(data: Data): Card[] {
  const { review, groups, threads, state } = data
  const items = (section: number | null) => review.items.filter((item) => item.section === section).map((item): Card => ({ key: `item-${item.id}`, label: `${item.id} ${item.title}`, kind: "item", item }))
  const parts = groups.flatMap((_, group): Card[] => {
    const label = review.sections[group] ? `${group + 1}. ${review.sections[group].title}` : review.sections.length ? "Code in no part" : "The diff"
    return [{ key: `part-${group}`, label, kind: "part", group }, ...items(group)]
  })
  return [
    ...(review.summary ? [{ key: "summary", label: "Summary", kind: "summary" } as const] : []),
    ...parts,
    ...items(null),
    ...threads.map((thread): Card => ({ key: `thread-${thread.id}`, label: `${thread.notes[0]?.[0] ?? "Thread"} on ${place(thread.file, thread.line)}`, kind: "thread", thread })),
    ...state.notes.map((note, n): Card => ({ key: `note-${n}`, label: `Your comment on ${note.item ?? place(note.file, note.line)}`, kind: "note", note: n })),
  ]
}

function locate(data: Data, file: string | null, line: number | null, old: boolean) {
  const found = data.groups.findIndex((group) =>
    group.some(([fi, hi]) => {
      const hunk = data.files[fi].hunks[hi]
      const [start, skip] = old ? [hunk.old_start, "+"] : [hunk.new_start, "-"]
      const length = hunk.lines.filter((text) => !text.startsWith(skip) && !text.startsWith("\\")).length
      return data.files[fi].path === file && line !== null && line >= start && line < start + length
    }),
  )
  return found < 0 ? undefined : found
}

function groupOf(data: Data, card: Card): number | undefined {
  if (card.kind === "part") return card.group
  if (card.kind === "item") return card.item.section ?? undefined
  if (card.kind === "thread") return locate(data, card.thread.file, card.thread.line, card.thread.old)
  if (card.kind === "note") {
    const note = data.state.notes[card.note]
    return locate(data, note.file, note.line, note.old) ?? data.review.items.find((item) => item.id === note.item)?.section ?? undefined
  }
}

function anchorOf(data: Data, card: Card): { file: string; line: number; old: boolean } | undefined {
  const at = (file: string | null | undefined, line: number | null | undefined, old = false) => (file && line ? { file, line, old } : undefined)
  if (card.kind === "item") return at(card.item.at?.file, card.item.at?.line)
  if (card.kind === "thread") return at(card.thread.file, card.thread.line, card.thread.old)
  if (card.kind === "note") {
    const note = data.state.notes[card.note]
    const item = data.review.items.find((item) => item.id === note.item)
    return at(note.file, note.line, note.old) ?? at(item?.at?.file, item?.at?.line)
  }
}

function titleOf(data: Data, cards: Card[], card: Card) {
  const sections = data.review.sections.length
  const what = {
    summary: () => "summary",
    part: () => (card.kind === "part" && card.group < sections ? `part ${card.group + 1} of ${sections}` : "code in no part"),
    item: () => (card.kind === "item" ? `${KINDS[card.item.kind]} ${card.item.id}` : ""),
    thread: () => "already said on the merge request",
    note: () => "your comment",
  }[card.kind]()
  return `${cards.indexOf(card) + 1} of ${cards.length}${data.review.working.trim() ? "+" : ""} · ${what}`
}

export { anchorOf, cardsOf, groupOf, place, titleOf }
export type { Card }
