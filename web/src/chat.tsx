import { Button } from "@ui-sdk/react/button"
import { Textarea } from "@ui-sdk/react/textarea"
import { ArrowUp, Minus, PanelRight, PictureInPicture2, ShieldCheck, Square, Workflow, X } from "lucide-react"
import { useEffect, useRef } from "react"
import type { PointerEvent as Press, RefObject } from "react"
import type { ChatEvent } from "../server/chat.ts"
import { Reply } from "./text.tsx"

type Entry = Exclude<ChatEvent, { kind: "delta" | "page" | "state" | "tool_result" | "answered" }> & { result?: { error: boolean; text: string }; allow?: boolean }
type Talk = { entries: Entry[]; draft: string; busy: boolean }

const quiet: Talk = { entries: [], draft: "", busy: false }

function talk(state: Talk, event: ChatEvent): Talk {
  const patch = (id: string, change: Partial<Entry>) => state.entries.map((entry) => ("id" in entry && entry.id === id ? ({ ...entry, ...change } as Entry) : entry))
  switch (event.kind) {
    case "delta":
      return { ...state, draft: state.draft + event.text }
    case "state":
      return { ...state, busy: event.busy, draft: event.busy ? state.draft : "" }
    case "tool_result":
      return { ...state, entries: patch(event.id, { result: { error: event.error, text: event.text } }) }
    case "answered":
      return { ...state, entries: patch(event.id, { allow: event.allow }) }
    case "page":
      return state
    default:
      return { ...state, draft: event.kind === "text" ? "" : state.draft, entries: [...state.entries, event] }
  }
}

function summary(name: string, input: unknown) {
  const given = (input ?? {}) as Record<string, unknown>
  const detail = given.file_path ?? given.command ?? given.pattern ?? given.description ?? given.url ?? given.query
  const tool = name.startsWith("mcp__review_page__") ? `page ${name.slice("mcp__review_page__".length)}` : name
  return typeof detail === "string" ? `${tool} ${detail}` : tool
}

function Message({ entry, answer }: { entry: Entry; answer: (id: string, allow: boolean) => void }) {
  switch (entry.kind) {
    case "user":
      return (
        <div className="said mine">
          {entry.about && <span className="about">about {entry.about}</span>}
          {entry.text}
        </div>
      )
    case "text":
      return <Reply text={entry.text} />
    case "tool":
      return (
        <details className="tool" data-error={entry.result?.error || undefined} data-running={entry.result ? undefined : true}>
          <summary>{summary(entry.name, entry.input)}</summary>
          <pre>{JSON.stringify(entry.input, null, 2)}</pre>
          {entry.result && <pre>{entry.result.text}</pre>}
        </details>
      )
    case "ask":
      return (
        <div className="ask" data-agent-hidden>
          <strong>{entry.title}</strong>
          <pre>{JSON.stringify(entry.input, null, 2)}</pre>
          {entry.allow === undefined ? (
            <footer>
              <Button onClick={() => answer(entry.id, false)}>Decline</Button>
              <Button data-variant="primary" onClick={() => answer(entry.id, true)}>Allow</Button>
            </footer>
          ) : (
            <em>{entry.allow ? "Allowed" : "Declined"}</em>
          )}
        </div>
      )
    case "error":
      return <div className="said problem">{entry.text}</div>
  }
}

type Props = {
  state: Talk
  about?: string
  docked: boolean
  text: string
  setText: (text: string) => void
  input: RefObject<HTMLTextAreaElement | null>
  grab?: (event: Press) => void
  send: (text: string, mode: string) => void
  answer: (id: string, allow: boolean) => void
  interrupt: () => void
  dock: (docked: boolean) => void
  minimize: () => void
  close: () => void
}

function Chat({ state, about, docked, text, setText, input, grab, send, answer, interrupt, dock, minimize, close }: Props) {
  const end = useRef<HTMLDivElement>(null)
  useEffect(() => void end.current?.scrollIntoView({ block: "end" }), [state.entries.length, state.draft])
  const submit = () => {
    if (!text.trim()) return
    send(text, "ask")
    setText("")
  }
  return (
    <aside className="chat" data-agent-hidden aria-label="The review's agent">
      <header onPointerDown={grab} data-grab={grab ? true : undefined}>
        <strong>Agent</strong>
        {state.busy && <span className="working">working</span>}
        <span className="grow" />
        <Button aria-label="Minimize the agent" title="Minimize" onClick={minimize}><Minus /></Button>
        <Button aria-label={docked ? "Detach the agent from the side" : "Dock the agent at the side"} title={docked ? "Detach" : "Dock"} onClick={() => dock(!docked)}>
          {docked ? <PictureInPicture2 /> : <PanelRight />}
        </Button>
        <Button aria-label="Close the agent" title="Close" onClick={close}><X /></Button>
      </header>
      <div className="talk">
        {state.entries.length === 0 && !state.busy && <p className="hint">This review's agent. It wrote the guide, so it already knows the change. Ask it about the card you are on.</p>}
        {state.entries.map((entry, n) => (
          <Message key={"id" in entry ? entry.id : n} entry={entry} answer={answer} />
        ))}
        {state.draft && <Reply text={state.draft} />}
        {state.busy && !state.draft && <div className="working">working</div>}
        <div ref={end} />
      </div>
      <form
        onSubmit={(event) => {
          event.preventDefault()
          submit()
        }}
      >
        <Textarea
          ref={input}
          aria-label="Message the agent"
          placeholder={about ? `Ask about ${about}` : "Ask about this review"}
          value={text}
          onValueChange={setText}
          rows={3}
          onKeyDown={(event) => {
            if (event.key === "Enter" && !event.shiftKey) {
              event.preventDefault()
              submit()
            }
          }}
        />
        <footer>
          <Button onClick={() => send("", "verify")}><ShieldCheck />Verify{about ? ` ${about}` : ""}</Button>
          <Button onClick={() => send("", "artifact")}><Workflow />Diagram</Button>
          <span />
          {state.busy ? <Button onClick={interrupt}><Square />Stop</Button> : <Button type="submit" data-variant="primary"><ArrowUp />Send</Button>}
        </footer>
      </form>
    </aside>
  )
}

export { Chat, quiet, talk }
export type { Talk }
