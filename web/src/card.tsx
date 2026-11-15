import { Button } from "@ui-sdk/react/button"
import { Collapsible, CollapsiblePanel, CollapsibleTrigger } from "@ui-sdk/react/collapsible"
import { Field, FieldLabel } from "@ui-sdk/react/field"
import { Textarea } from "@ui-sdk/react/textarea"
import { Toggle } from "@ui-sdk/react/toggle"
import { Check, Quote, Trash2, X } from "lucide-react"
import { useState } from "react"
import type { RefObject } from "react"
import { place } from "./cards.ts"
import type { Card } from "./cards.ts"
import type { Line } from "./diff.tsx"
import { Prose } from "./text.tsx"
import type { Data, Evidence, Item, Note, State } from "./types.ts"

type Props = {
  data: Data
  card: Card
  picked?: Line
  comment: RefObject<HTMLTextAreaElement | null>
  go: (key: string) => void
  save: (change: (state: State) => void) => void
}

const TIERS: Record<string, string> = { core: "the core change", supporting: "supporting code", low: "low signal, skim it" }
const PROVENANCE = { observed: "observed in a run", checked: "verified by a tool", inferred: "reasoned from reading" }
const STATUS = { confirmed: "confirmed by a run", unreproduced: "checked, did not reproduce", resolved: "fixed at this head" }
const PICTURE = /\.(png|jpe?g|gif|webp|svg)$/i
const VIDEO = /\.(mp4|webm|mov)$/i
const blank: Note = { text: "", file: null, line: null, start: null, old: false, item: null, reply_to: null, posted: false }

function label(item: Item) {
  if (item.kind === "decision") return "Decision"
  if (item.kind === "good") return "Good call"
  return { blocker: "Blocker", fix: "Should fix", "should fix": "Should fix", suggestion: "Suggestion", question: "Question" }[item.severity] ?? "Finding"
}

function Compose({ title, start = "", input, onSave }: { title: string; start?: string; input?: RefObject<HTMLTextAreaElement | null>; onSave: (text: string) => void }) {
  const [text, setText] = useState(start)
  return (
    <Field className="compose">
      <FieldLabel>{title}</FieldLabel>
      <Textarea ref={input} value={text} onValueChange={setText} rows={3} />
      <Button
        data-variant="primary"
        disabled={text.trim() === start.trim()}
        onClick={() => {
          onSave(text.trim())
          setText(start && text.trim())
        }}
      >
        Save comment
      </Button>
    </Field>
  )
}

function Proof({ evidence, n }: { evidence: Evidence; n: number }) {
  const src = evidence.path ? `/bundle/${evidence.path}` : undefined
  const inferred = evidence.provenance === "inferred"
  return (
    <li className="evidence">
      <header>
        <span className="number">{n + 1}</span>
        <span className="provenance" data-provenance={evidence.provenance}>{PROVENANCE[evidence.provenance]}</span>
      </header>
      <p className="caption">{evidence.caption}</p>
      {src && PICTURE.test(src) && <a href={src} target="_blank" rel="noreferrer"><img src={src} alt={evidence.caption} /></a>}
      {src && VIDEO.test(src) && <video src={src} controls muted />}
      {src && !PICTURE.test(src) && !VIDEO.test(src) && <a href={src} target="_blank" rel="noreferrer">Open {evidence.path}</a>}
      {(evidence.repro || evidence.text) && (
        <Collapsible>
          <CollapsibleTrigger>Details of evidence {n + 1}</CollapsibleTrigger>
          <CollapsiblePanel>
            {evidence.repro && (
              <>
                <h5>{inferred ? (evidence.path ? "To redraw the artifact" : "Command used") : "To reproduce"}</h5>
                <pre>$ {evidence.repro}</pre>
              </>
            )}
            {evidence.text && (
              <>
                <h5>{inferred ? "The reasoning" : "What it showed"}</h5>
                <pre>{evidence.text}</pre>
              </>
            )}
          </CollapsiblePanel>
        </Collapsible>
      )}
    </li>
  )
}

function Ramp({ data }: { data: Data }) {
  const count = (group: number, sign: string) => data.groups[group].reduce((sum, [file, hunk]) => sum + data.files[file].hunks[hunk].lines.filter((line) => line.startsWith(sign)).length, 0)
  const parts = data.groups.map((_, group) => ({ added: count(group, "+"), removed: count(group, "-") }))
  const most = Math.max(1, ...parts.map((part) => part.added + part.removed))
  const total = (key: "added" | "removed") => parts.reduce((sum, part) => sum + part[key], 0)
  return (
    <>
      <div className="ramp" aria-hidden>
        {parts.map((part, group) => {
          const share = (part.added + part.removed) / most
          return <i key={group} style={{ height: `${Math.max(4, share * 100)}%`, opacity: 0.3 + share * 0.7 }} />
        })}
      </div>
      <p className="readout">
        <span>Lines changed / part</span>
        <b>+{total("added")} −{total("removed")}</b>
      </p>
      <dl className="stats">
        {[
          [data.review.sections.length, "parts"],
          [data.review.items.filter((item) => item.kind === "finding").length, "findings"],
          [data.review.items.filter((item) => item.kind === "decision").length, "decisions"],
          [data.files.length, "files"],
        ].map(([value, name]) => (
          <div key={name}>
            <dt>{name}</dt>
            <dd>{value}</dd>
          </div>
        ))}
      </dl>
    </>
  )
}

function Summary({ data, go }: Pick<Props, "data" | "go">) {
  const { review } = data
  const lead = data.product && review.plain.trim()
  return (
    <>
      <div className="hero">
        <h1>{review.title}</h1>
        <Ramp data={data} />
      </div>
      {lead && <Prose text={review.plain} />}
      {lead && <h2>In detail</h2>}
      <Prose text={review.summary} />
      {review.map && (
        <>
          <h2>Where it sits</h2>
          <ul className="proofs"><Proof evidence={review.map} n={0} /></ul>
        </>
      )}
      <h2>Read in this order</h2>
      <p className="hint">The guide splits the diff into parts, the core of the change first. A finding sits under the part whose code it is about, so a part with none had nothing to raise.</p>
      <ol className="order">
        {review.sections.map((part, g) => {
          const notes = [TIERS[part.tier] ?? part.tier, ...review.items.filter((item) => item.section === g).map((item) => item.id)].filter(Boolean)
          return (
            <li key={g}>
              <button type="button" onClick={() => go(`part-${g}`)}>{part.title}</button>
              <span className="hint">{notes.join(" · ")}</span>
            </li>
          )
        })}
      </ol>
      {data.groups.length > review.sections.length && <p className="hint">Then the hunks the guide put in no section.</p>}
    </>
  )
}

function Part({ data, group }: { data: Data; group: number }) {
  const section = data.review.sections[group]
  const files = new Set(data.groups[group].map(([file]) => file)).size
  if (!section) {
    const text = data.review.sections.length ? "These hunks are in the diff, but the guide put them in no section. Nothing vouches for them: read them." : "No guide for this merge request yet. Ask the agent to write one. This is its plain diff."
    return (
      <>
        <h1>{data.review.sections.length ? "Code in no part" : data.review.title}</h1>
        <Prose text={text} />
      </>
    )
  }
  const raised = data.review.items.some((item) => item.section === group)
  return (
    <>
      <h1>{section.title}</h1>
      <p className="meta">
        {section.tier && <span className="tier" data-tier={section.tier}>{TIERS[section.tier] ?? section.tier}</span>}
        {files} file{files === 1 ? "" : "s"}
        {!raised && " · nothing raised here"}
      </p>
      <Prose text={section.why} />
    </>
  )
}

function Finding({ data, item, picked, comment, save }: Omit<Props, "card" | "go"> & { item: Item }) {
  const verdict = data.state.verdicts[item.id]
  const decision = item.kind === "decision"
  const mine = data.state.notes.map((note, n) => ({ note, n })).filter(({ note }) => note.item === item.id)
  const commented = mine.some(({ note }) => !note.posted)
  const quoted = data.quoted[item.id]
  const fate = verdict === undefined ? "" : quoted ? (commented ? "goes out quoted, with your comment" : "goes out quoted") : commented ? "only your comment goes out" : "nothing goes out"

  function mark(next: "agree" | "dismiss") {
    save((state) => {
      const cleared = state.verdicts[item.id] === next
      const wantsReason = !cleared && (item.kind === "finding" || (decision && next === "dismiss"))
      if (!wantsReason) state.notes = state.notes.filter((note) => note.posted || note.item !== item.id || note.file !== null || note.reply_to !== null)
      if (cleared) delete state.verdicts[item.id]
      else state.verdicts[item.id] = next
      delete state.quotes[item.id]
      state.finished = null
    })
  }

  const target = picked ? { ...blank, file: picked.file, line: picked.line, old: picked.old } : { ...blank, item: item.id, file: item.at?.file ?? null, line: item.at?.line ?? null }
  return (
    <>
      <p className="meta">
        <span className="kind" data-kind={item.kind} data-severity={item.severity}>{label(item)}</span>
        {item.id}
        {item.status && <span className="status" data-status={item.status}>{STATUS[item.status]}</span>}
        {data.links[item.id] && <a href={data.links[item.id]} target="_blank" rel="noreferrer">{place(item.at?.file ?? null, item.at?.line ?? null)}</a>}
      </p>
      <h1>{item.title}</h1>
      {item.kind !== "good" && (
        <div className="verdict">
          <Toggle aria-label={`${decision ? "Accept" : "Agree with"} ${item.id}`} data-verdict="agree" pressed={verdict === "agree"} onPressedChange={() => mark("agree")}>
            <Check />{decision ? "Accept" : "Agree"}
          </Toggle>
          <Toggle aria-label={`${decision ? "Push back on" : "Dismiss"} ${item.id}`} data-verdict="dismiss" pressed={verdict === "dismiss"} onPressedChange={() => mark("dismiss")}>
            <X />{decision ? "Push back" : "Dismiss"}
          </Toggle>
          <span className="hint">{fate}</span>
          {verdict !== undefined && (
            <Toggle aria-label={`Quote ${item.id} above my comment`} className="quote" pressed={quoted} onPressedChange={(pressed) => save((state) => void (state.quotes[item.id] = pressed))}>
              <Quote />Quote it
            </Toggle>
          )}
        </div>
      )}
      {data.product && item.plain.trim() && (
        <>
          <Prose text={item.plain} />
          <h2>In detail</h2>
        </>
      )}
      <Prose text={item.body} />
      {mine.length > 0 && <h2>Your comments on it</h2>}
      {mine.map(({ note, n }) => (
        <p key={n} className="note" data-posted={note.posted || undefined}>{note.text}</p>
      ))}
      <h2>Evidence ({item.evidence.length || "none"})</h2>
      {item.evidence.length === 0 && <p className="hint">None recorded. Ask the agent to verify it or draw it.</p>}
      <ul className="proofs">
        {item.evidence.map((evidence, n) => (
          <Proof key={n} evidence={evidence} n={n} />
        ))}
      </ul>
      <Compose
        key={picked ? `${picked.file}:${picked.line}` : item.id}
        input={comment}
        title={picked ? `Comment on ${picked.file}:${picked.line}` : `Comment on ${item.id}`}
        onSave={(text) =>
          save((state) => {
            state.notes.push({ ...target, text })
            state.finished = null
          })
        }
      />
    </>
  )
}

function CardView({ data, card, picked, comment, go, save }: Props) {
  const add = (note: Partial<Note>) => (text: string) =>
    save((state) => {
      state.notes.push({ ...blank, ...note, text })
      state.finished = null
    })
  const lineComment = picked && <Compose key={`${picked.file}:${picked.line}`} input={comment} title={`Comment on ${picked.file}:${picked.line}`} onSave={add({ file: picked.file, line: picked.line, old: picked.old })} />
  switch (card.kind) {
    case "summary":
      return (
        <>
          <Summary data={data} go={go} />
          <Compose input={comment} title="Comment on the whole merge request" onSave={add({})} />
        </>
      )
    case "part":
      return (
        <>
          <Part data={data} group={card.group} />
          {lineComment}
        </>
      )
    case "item":
      return <Finding data={data} item={card.item} picked={picked} comment={comment} save={save} />
    case "thread": {
      const { thread } = card
      const replies = data.state.notes.filter((note) => note.reply_to === thread.id)
      return (
        <>
          <p className="meta">{thread.resolved ? "resolved" : "open"} · on the merge request</p>
          <h1>Thread on {place(thread.file, thread.line)}</h1>
          {thread.notes.map(([author, body, when], n) => (
            <div key={n} className="said">
              <strong>{author}</strong> <time>{when.slice(0, 10)}</time>
              <Prose text={body} />
            </div>
          ))}
          {replies.map((reply, n) => (
            <p key={n} className="note" data-posted={reply.posted || undefined}>{reply.text}</p>
          ))}
          <Compose input={comment} title="Reply in this thread" onSave={add({ reply_to: thread.id })} />
        </>
      )
    }
    case "note": {
      const note = data.state.notes[card.note]
      if (!note) return null
      return (
        <>
          <p className="meta">{note.posted ? "posted to the merge request" : "not published yet"}</p>
          <h1>Your comment on {note.reply_to ? "a thread" : (note.item ?? place(note.file, note.line))}</h1>
          {note.posted ? (
            <Prose text={note.text} />
          ) : (
            <>
              <Compose
                key={card.key}
                input={comment}
                title="Edit your comment"
                start={note.text}
                onSave={(text) =>
                  save((state) => {
                    if (text) state.notes[card.note].text = text
                    else state.notes.splice(card.note, 1)
                    state.finished = null
                  })
                }
              />
              <Button onClick={() => save((state) => void state.notes.splice(card.note, 1))}><Trash2 />Delete this comment</Button>
            </>
          )}
        </>
      )
    }
  }
}

export { CardView }
