import { Button } from "@ui-sdk/react/button"
import { Dialog, DialogBackdrop, DialogContent, DialogTitle } from "@ui-sdk/react/dialog"
import { Menu, MenuContent, MenuItem, MenuTrigger } from "@ui-sdk/react/menu"
import { OverlayHost } from "@ui-sdk/react/overlays"
import { Resizable, ResizableHandle, ResizablePanel } from "@ui-sdk/react/resizable"
import { Tabs, TabsList, TabsPanel, TabsTab } from "@ui-sdk/react/tabs"
import { toast, Toast, ToastClose, ToastDescription, ToastTitle } from "@ui-sdk/react/toast"
import { Toggle } from "@ui-sdk/react/toggle"
import { Bot, ChevronDown, FileDiff, List, MessageCircle, Pin, Send, ShieldCheck, Workflow, X } from "lucide-react"
import { useCallback, useEffect, useMemo, useReducer, useRef, useState } from "react"
import type { PointerEvent as Press } from "react"
import type { ChatEvent } from "../server/chat.ts"
import { page } from "./agent.ts"
import { call } from "./api.ts"
import { CardView } from "./card.tsx"
import { anchorOf, cardsOf, groupOf, titleOf } from "./cards.ts"
import { Chat, quiet, talk } from "./chat.tsx"
import { Diff } from "./diff.tsx"
import { Float } from "./float.tsx"
import type { Line } from "./diff.tsx"
import { Publish } from "./publish.tsx"
import type { Data, State } from "./types.ts"

const typing = (target: EventTarget | null) => target instanceof HTMLElement && (target.closest("input, textarea, [contenteditable], dialog, [role=dialog], [role=alertdialog]") !== null)

function App() {
  const [data, setData] = useState<Data>()
  const [problem, setProblem] = useState("")
  const [current, setCurrent] = useState(() => sessionStorage.getItem("revui-card") ?? "")
  const [picked, setPicked] = useState<Line>()
  const [showDiff, setShowDiff] = useState(true)
  const [showChat, setShowChat] = useState(() => innerWidth >= 1100)
  const [docked, setDocked] = useState(() => localStorage.getItem("revui-agent-docked") !== "false")
  const [minimized, setMinimized] = useState(false)
  const [draft, setDraft] = useState("")
  const [publishing, setPublishing] = useState(false)
  const [outline, setOutline] = useState(() => localStorage.getItem("revui-outline") === "true")
  const [contents, setContents] = useState(false)
  const [focus, setFocus] = useState<"card" | "diff">("card")
  const [proof, setProof] = useState(0)
  const [chat, dispatch] = useReducer((state: typeof quiet, event: ChatEvent | "reset") => (event === "reset" ? quiet : talk(state, event)), quiet)
  const comment = useRef<HTMLTextAreaElement>(null)
  const message = useRef<HTMLTextAreaElement>(null)

  const load = useCallback(() => call<Data>("/api/review").then(setData, (error) => setProblem(String(error.message ?? error))), [])

  useEffect(() => {
    load()
    call<Data>("/api/threads", "POST").then(setData, () => {})
    const events = new EventSource("/api/events")
    events.onopen = () => dispatch("reset")
    events.onmessage = async (message) => {
      const event: ChatEvent | { kind: "review" } = JSON.parse(message.data)
      if (event.kind === "review") return void load()
      if (event.kind === "page") {
        const answer = await Promise.resolve(page(event.tool, event.args)).catch((error) => ({ error: String(error) }))
        return void call("/api/chat/answer", "POST", { id: event.id, answer: answer ?? null })
      }
      dispatch(event)
    }
    return () => events.close()
  }, [load])

  const cards = useMemo(() => (data ? cardsOf(data) : []), [data])
  const card = cards.find((card) => card.key === current) ?? cards[0]
  const group = data && card ? groupOf(data, card) : undefined

  const go = useCallback((key: string) => {
    sessionStorage.setItem("revui-card", key)
    setCurrent(key)
    setPicked(undefined)
    setFocus("card")
    setProof(0)
  }, [])

  useEffect(() => {
    document.querySelectorAll(".card .evidence").forEach((each, n) => each.toggleAttribute("data-current", focus === "card" && n === proof))
  })

  const save = useCallback(
    (change: (state: State) => void) => {
      if (!data) return
      const next = structuredClone(data.state)
      change(next)
      call<Data>("/api/state", "PUT", next).then(setData, (error) => toast.error(`Could not save: ${error.message}`))
    },
    [data],
  )

  const about = card?.kind === "item" ? card.item.id : card?.kind === "part" ? String(card.group + 1) : undefined
  const send = (text: string, mode: string) => call("/api/chat", "POST", { text, mode, on: about }).catch((error) => toast.error(error.message))

  const pane = (mode: string) =>
    call<{ ok: boolean; note: string; problem: string }>("/api/agent", "POST", { mode, on: about }).then(
      (ran) => (ran.ok ? toast(ran.note) : toast.error(ran.problem)),
      (error) => toast.error(error.message),
    )

  const pin = (pinned: boolean) => {
    localStorage.setItem("revui-outline", String(pinned))
    setOutline(pinned)
  }

  const openChat = useCallback((open: boolean) => {
    setShowChat(open)
    setMinimized(false)
  }, [])

  useEffect(() => {
    const press = (event: KeyboardEvent) => {
      if (data?.agent && (event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "j") {
        event.preventDefault()
        return openChat(!showChat || minimized)
      }
      if (contents || publishing || typing(event.target) || event.metaKey || event.altKey || !card) return
      const step = (delta: number) => cards[cards.indexOf(card) + delta] && go(cards[cards.indexOf(card) + delta].key)
      const verdict = (name: string) => document.querySelector<HTMLElement>(`[data-verdict=${name}]`)?.click()
      const scroller = document.querySelector(`[data-slot="resizable-panel"]#${focus}`)
      const proofs = [...document.querySelectorAll<HTMLElement>(".card .evidence")]
      const show = (n: number) => {
        setProof(n)
        proofs[n]?.scrollIntoView({ block: "nearest" })
      }
      const move = (delta: number, pane = focus) => {
        if (pane === "card") return proofs[proof + delta] ? show(proof + delta) : scroller?.scrollBy({ top: delta * 60 })
        const lines = [...document.querySelectorAll<HTMLElement>(".diff .line[data-line]")]
        const at = lines.findIndex((line) => line.matches("[data-picked]"))
        const next = lines[at < 0 ? Math.max(0, lines.findIndex((line) => line.matches("[data-anchor]"))) : Math.min(lines.length - 1, Math.max(0, at + delta))]
        if (!next) return
        setPicked({ file: next.dataset.file!, line: Number(next.dataset.line), old: next.dataset.old === "true" })
        next.scrollIntoView({ block: "nearest" })
      }
      const page = (way: number) => scroller?.scrollBy({ top: (way * scroller.clientHeight) / 2 })
      const keys: Record<string, () => unknown> = {
        ArrowRight: () => step(1),
        l: () => step(1),
        ArrowLeft: () => step(-1),
        h: () => step(-1),
        j: () => move(1),
        ArrowDown: () => move(1),
        k: () => move(-1),
        ArrowUp: () => move(-1),
        u: () => page(1),
        PageDown: () => page(1),
        U: () => page(-1),
        PageUp: () => page(-1),
        Tab: () => {
          if (event.shiftKey || !document.querySelector(".diff")) return
          if (focus === "diff") setPicked(undefined)
          setFocus(focus === "diff" ? "card" : "diff")
          if (focus === "card") move(0, "diff")
        },
        e: () => proofs.length && show((proof + 1) % proofs.length),
        o: () => proofs[proof]?.querySelector<HTMLElement>("a[href]")?.click(),
        Enter: () => proofs[proof]?.querySelector<HTMLElement>("a[href]")?.click(),
        v: () => (data?.agent ? send("", "verify") : pane("verify")),
        i: () => (data?.agent ? send("", "artifact") : pane("artifact")),
        m: () => data?.review.sources[0]?.url && open(data.review.sources[0].url),
        a: () => verdict("agree"),
        x: () => verdict("dismiss"),
        c: () => comment.current?.focus(),
        t: () => {
          if (!data?.agent) return pane("ask")
          openChat(true)
          requestAnimationFrame(() => message.current?.focus())
        },
        d: () => setShowDiff((shown) => !shown),
        b: () => setContents(true),
        B: () => pin(!outline),
        p: () => setPublishing(true),
      }
      if (event.ctrlKey && event.key === "o") {
        event.preventDefault()
        return proofs[proof]?.querySelector<HTMLElement>('[data-slot="collapsible-trigger"]')?.click()
      }
      if (event.ctrlKey || !keys[event.key] || (event.key === "Tab" && event.shiftKey)) return
      event.preventDefault()
      keys[event.key]()
    }
    window.addEventListener("keydown", press)
    return () => window.removeEventListener("keydown", press)
  }, [data?.agent, cards, card, go, openChat, showChat, minimized, outline, focus, proof, contents, publishing])

  if (problem) return <main className="fatal">{problem}</main>
  if (!data || !card) return <main className="fatal">Loading the review</main>

  const source = data.review.sources[0]
  const diffed = showDiff && group !== undefined && !data.product
  const marks = Object.keys(data.state.verdicts).length
  const chatAt = !data.agent || !showChat ? "closed" : minimized ? "minimized" : docked ? "docked" : "floating"
  const agent = (grab?: (event: Press) => void) => (
    <Chat
      state={chat}
      about={card.kind === "item" ? card.item.id : card.kind === "part" ? `part ${card.group + 1}` : undefined}
      docked={docked}
      text={draft}
      setText={setDraft}
      input={message}
      grab={grab}
      send={send}
      answer={(id, allow) => call("/api/chat/answer", "POST", { id, answer: allow })}
      interrupt={() => call("/api/chat/interrupt", "POST", {})}
      dock={(docked) => {
        localStorage.setItem("revui-agent-docked", String(docked))
        setDocked(docked)
      }}
      minimize={() => setMinimized(true)}
      close={() => openChat(false)}
    />
  )
  return (
    <Tabs className="app" orientation="vertical" value={card.key} onValueChange={(key) => key && go(key)}>
      <header className="bar">
        <svg className="brand" viewBox="0 0 111 111" role="img" aria-label="revui">
          <path d="M11 3.25H74.65L107.75 36.35V100" fill="none" stroke="#5B6EE1" strokeWidth="6.5" />
          <path d="M0 11H65L100 46V111H0Z" fill="#08090C" />
          <path d="M24 64L42 82L76 48" fill="none" stroke="#F3F4F8" strokeWidth="11" />
        </svg>
        <span className="title">{data.review.title}</span>
        {source?.reference && <a className="readout" href={source.url} target="_blank" rel="noreferrer">{source.repo}{source.reference}</a>}
        {data.review.working.trim() && <span className="working">{data.review.working}</span>}
        <span className="readout">{titleOf(data, cards, card)}</span>
        <span className="grow" />
        <Button aria-label="Contents" onClick={() => setContents(true)}><List /><span className="label">Contents</span></Button>
        <Toggle aria-label="Show the diff" pressed={showDiff} onPressedChange={setShowDiff}><FileDiff /><span className="label">Diff</span></Toggle>
        {data.agent && <Toggle aria-label="Show the agent" pressed={showChat} onPressedChange={openChat}><Bot /><span className="label">Agent</span></Toggle>}
        {!data.agent && (
          <Menu>
            <MenuTrigger aria-label="Agent"><Bot /><span className="label">Agent</span><ChevronDown /></MenuTrigger>
            <MenuContent align="end" sideOffset={6}>
              <MenuItem onSelect={() => pane("ask")}><MessageCircle />Ask{about ? ` about ${about}` : ""}</MenuItem>
              <MenuItem onSelect={() => pane("verify")}><ShieldCheck />Verify{about ? ` ${about}` : ""}</MenuItem>
              <MenuItem onSelect={() => pane("artifact")}><Workflow />Diagram</MenuItem>
            </MenuContent>
          </Menu>
        )}
        <Button data-variant="primary" aria-label="Publish" onClick={() => setPublishing(true)}>
          <Send /><span className="label">Publish</span>{data.outgoing || ""}
        </Button>
      </header>
      <Resizable className="panes" autoSaveId="revui-panes">
        {outline && <ResizablePanel id="outline" defaultSize={18} minSize={10}>
          <TabsList aria-label="Cards" className="outline">
            {cards.map((card) => (
              <TabsTab key={card.key} value={card.key} data-kind={card.kind} data-verdict={card.kind === "item" ? data.state.verdicts[card.item.id] : undefined}>
                {card.label}
              </TabsTab>
            ))}
          </TabsList>
          <p className="tally">{marks} of {data.review.items.filter((item) => item.kind !== "good").length} marked</p>
        </ResizablePanel>}
        {outline && <ResizableHandle aria-label="Resize the list of cards" />}
        <ResizablePanel id="card" defaultSize={diffed ? 32 : 60} minSize={20}>
          <TabsPanel value={card.key} className="card">
            <CardView data={data} card={card} picked={picked} comment={comment} go={go} save={save} />
          </TabsPanel>
        </ResizablePanel>
        {diffed && <ResizableHandle aria-label="Resize the diff" />}
        {diffed && (
          <ResizablePanel id="diff" defaultSize={30} minSize={15}>
            <Diff
              data={data}
              group={group}
              anchor={anchorOf(data, card)}
              picked={picked}
              onPick={(line) => {
                setFocus("diff")
                setPicked(line)
              }}
            />
          </ResizablePanel>
        )}
        {chatAt === "docked" && <ResizableHandle aria-label="Resize the agent" />}
        {chatAt === "docked" && (
          <ResizablePanel id="chat" defaultSize={22} minSize={14}>
            {agent()}
          </ResizablePanel>
        )}
      </Resizable>
      <footer className="keys">
        l next · h back · {focus === "diff" ? "j k line · c comment on the line · tab card" : `j k ${card.kind === "item" ? "evidence" : "scroll"}${card.kind === "item" ? " · a agree · x dismiss" : ""} · c comment${diffed ? " · tab diff" : ""}`} · t ask · v verify · i diagram · d diff · b contents · p publish
      </footer>
      {chatAt === "floating" && <Float label="The review's agent">{agent}</Float>}
      {chatAt === "minimized" && (
        <div className="minimized" data-agent-hidden>
          <button type="button" aria-label="Restore the agent" onClick={() => setMinimized(false)}>
            <strong>Agent</strong>
            {chat.busy && <span className="working">working</span>}
          </button>
          <button type="button" aria-label="Close the agent" onClick={() => openChat(false)}><X /></button>
        </div>
      )}
      <Dialog open={contents} onOpenChange={setContents}>
        <DialogBackdrop />
        <DialogContent className="contents" initialFocus={() => document.querySelector<HTMLElement>(".contents [data-current]") ?? undefined}>
          <header>
            <DialogTitle>Contents</DialogTitle>
            <Toggle aria-label="Keep the list beside the card" title="Keep the list beside the card (B)" pressed={outline} onPressedChange={pin}><Pin /></Toggle>
          </header>
          <div
            className="outline"
            onKeyDown={(event) => {
              const step = { j: 1, ArrowDown: 1, k: -1, ArrowUp: -1 }[event.key]
              const row = step && document.activeElement?.[step > 0 ? "nextElementSibling" : "previousElementSibling"]
              if (!(row instanceof HTMLElement)) return
              event.preventDefault()
              row.focus()
            }}
          >
            {cards.map((each) => (
              <button
                key={each.key}
                type="button"
                data-slot="tabs-tab"
                data-kind={each.kind}
                data-current={each.key === card.key || undefined}
                onClick={() => {
                  go(each.key)
                  setContents(false)
                }}
              >
                {each.label}
              </button>
            ))}
          </div>
        </DialogContent>
      </Dialog>
      <Publish data={data} open={publishing} onOpenChange={setPublishing} onPublished={() => call<Data>("/api/threads", "POST").then(setData, () => {})} />
      <OverlayHost
        toast={{
          children: () => (
            <Toast>
              <div>
                <ToastTitle />
                <ToastDescription />
              </div>
              <ToastClose aria-label="Dismiss"><X /></ToastClose>
            </Toast>
          ),
        }}
      />
    </Tabs>
  )
}

export { App }
