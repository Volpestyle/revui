import { createSdkMcpServer, getSessionInfo, getSessionMessages, query, tool } from "@anthropic-ai/claude-agent-sdk"
import type { CanUseTool, Query, SDKMessage, SDKUserMessage } from "@anthropic-ai/claude-agent-sdk"
import { agentTools } from "@ui-sdk/core/agent"
import { spawn } from "node:child_process"
import { randomUUID } from "node:crypto"
import { readFileSync, writeFileSync } from "node:fs"
import { join } from "node:path"
import { z } from "zod"

type ChatEvent =
  | { kind: "user"; id: string; text: string; about?: string }
  | { kind: "text"; id: string; text: string }
  | { kind: "delta"; text: string }
  | { kind: "tool"; id: string; name: string; input: unknown }
  | { kind: "tool_result"; id: string; error: boolean; text: string }
  | { kind: "ask"; id: string; tool: string; title: string; input: unknown }
  | { kind: "answered"; id: string; allow: boolean }
  | { kind: "page"; id: string; tool: string; args: unknown }
  | { kind: "state"; busy: boolean }
  | { kind: "error"; text: string }

type Saved = { agent: string; handle: string; id: string; cwd: string }

const PAGE = "review_page"
const PAGE_WAIT = 6 * 60_000

function flatten(content: unknown): string {
  if (typeof content === "string") return content
  if (!Array.isArray(content)) return ""
  return content.map((block) => (block?.type === "text" ? block.text : "")).join("")
}

function eventsOf(role: string, message: unknown, id: string): ChatEvent[] {
  const content = (message as { content?: unknown })?.content
  if (typeof content === "string") return role === "user" ? [{ kind: "user", id, text: content }] : [{ kind: "text", id, text: content }]
  if (!Array.isArray(content)) return []
  return content.flatMap((block, n): ChatEvent[] => {
    switch (block?.type) {
      case "text":
        return [{ kind: role === "user" ? "user" : "text", id: `${id}-${n}`, text: block.text }]
      case "tool_use":
        return [{ kind: "tool", id: block.id, name: block.name, input: block.input }]
      case "tool_result":
        return [{ kind: "tool_result", id: block.tool_use_id, error: block.is_error === true, text: flatten(block.content).slice(0, 4000) }]
      default:
        return []
    }
  })
}

/** The config's `claude` is a shell command, often `env -u NAME claude`: the session files are read here with the same variables it runs with. */
function adopt(command: string) {
  const words = command.trim().split(/\s+/)
  if (words.shift() !== "env") return
  for (let word = words.shift(); word; word = words.shift()) {
    if (word === "-u") delete process.env[words.shift() ?? ""]
    else if (/^\w+=/.test(word)) process.env[word.slice(0, word.indexOf("="))] = word.slice(word.indexOf("=") + 1)
    else return
  }
}

class Chat {
  private log: ChatEvent[] = []
  private listeners = new Set<(event: ChatEvent) => void>()
  private inbox: SDKUserMessage[] = []
  private wake: (() => void) | undefined
  private running: Query | undefined
  private waiting = new Map<string, (answer: unknown) => void>()
  private loaded: Promise<void>
  private busy = false
  private pages = 0
  private dir: string
  private claude: string
  fresh = true

  constructor(dir: string, claude: string) {
    this.dir = dir
    this.claude = claude
    adopt(claude)
    this.loaded = this.history()
  }

  private saved(): Saved | undefined {
    try {
      return JSON.parse(readFileSync(join(this.dir, "session.json"), "utf8"))
    } catch {
      return undefined
    }
  }

  private async resumable(): Promise<Saved | undefined> {
    const saved = this.saved()
    if (!saved?.id || !saved.cwd) return undefined
    return (await getSessionInfo(saved.id, { dir: saved.cwd })) ? saved : undefined
  }

  private async history() {
    const saved = await this.resumable()
    if (!saved) return
    const messages = await getSessionMessages(saved.id, { dir: saved.cwd })
    const main = messages.filter((message) => message.parent_tool_use_id === null && message.type !== "system")
    this.log = main.flatMap((message) => eventsOf(message.type, message.message, message.uuid))
    this.fresh = this.log.length === 0
  }

  private emit(event: ChatEvent) {
    if (event.kind !== "delta" && event.kind !== "page" && event.kind !== "state") this.log.push(event)
    for (const listener of this.listeners) listener(event)
  }

  async subscribe(listener: (event: ChatEvent) => void, page: boolean) {
    await this.loaded
    for (const event of this.log) listener(event)
    listener({ kind: "state", busy: this.busy })
    this.listeners.add(listener)
    this.pages += page ? 1 : 0
    return () => {
      this.listeners.delete(listener)
      this.pages -= page ? 1 : 0
    }
  }

  private ask<T>(event: ChatEvent & { id: string }, signal?: AbortSignal): Promise<T | undefined> {
    return new Promise((resolve) => {
      const done = (answer: unknown) => {
        this.waiting.delete(event.id)
        resolve(answer as T | undefined)
      }
      this.waiting.set(event.id, done)
      signal?.addEventListener("abort", () => done(undefined))
      this.emit(event)
    })
  }

  answer(id: string, answer: unknown) {
    this.waiting.get(id)?.(answer)
  }

  private permit: CanUseTool = async (name, input, options) => {
    if (options.mcpServer?.source === "sdk" && options.mcpServer.name === PAGE) return { behavior: "allow", updatedInput: input }
    const id = randomUUID()
    const allow = await this.ask<boolean>({ kind: "ask", id, tool: name, title: options.title ?? name, input }, options.signal)
    this.emit({ kind: "answered", id, allow: allow === true })
    return allow === true ? { behavior: "allow", updatedInput: input } : { behavior: "deny", message: "The reviewer declined this." }
  }

  private page = async (name: string, args: unknown) => {
    const text = (value: string) => ({ content: [{ type: "text" as const, text: value }] })
    if (this.pages === 0) return { ...text("No review page is open in a browser, so there is nothing to read or operate."), isError: true }
    const timeout = AbortSignal.timeout(PAGE_WAIT)
    const result = await this.ask<unknown>({ kind: "page", id: randomUUID(), tool: name, args }, timeout)
    return result === undefined ? { ...text("The review page did not answer."), isError: true } : text(JSON.stringify(result))
  }

  private pageServer() {
    const [snapshot, act, find] = agentTools
    const step = z.object({ ref: z.string(), action: z.string(), args: z.array(z.unknown()).optional() })
    return createSdkMcpServer({
      name: PAGE,
      instructions:
        "The revui page the reviewer has open on this review: its cards, verdict buttons, comment box and publish dialog. Use it to take the reviewer to a card or to fill in a draft for them. Publishing needs the reviewer: never look for a way around a step that fails with \"Needs a person\".",
      tools: [
        tool(snapshot.name, snapshot.description, {}, (args) => this.page("snapshot", args)),
        tool(act.name, act.description, { steps: z.array(step) }, (args) => this.page("act", args)),
        tool(find.name, find.description, { text: z.string().optional(), within: z.string().optional(), kind: z.string().optional() }, (args) => this.page("query", args)),
      ],
    })
  }

  private async *prompts(): AsyncGenerator<SDKUserMessage> {
    for (;;) {
      const next = this.inbox.shift()
      if (next) yield next
      else await new Promise<void>((resolve) => (this.wake = resolve))
    }
  }

  private async start() {
    const saved = await this.resumable()
    const session = saved ?? { agent: "agent", handle: "", id: randomUUID(), cwd: process.cwd() }
    if (!saved) writeFileSync(join(this.dir, "session.json"), JSON.stringify(session))
    this.running = query({
      prompt: this.prompts(),
      options: {
        cwd: session.cwd,
        ...(saved ? { resume: session.id } : { sessionId: session.id }),
        pathToClaudeCodeExecutable: "claude",
        spawnClaudeCodeProcess: ({ args, cwd, env, signal }) => spawn("/bin/sh", ["-c", `exec ${this.claude} "$@"`, "sh", ...args], { cwd, env, signal, stdio: ["pipe", "pipe", "inherit"] }),
        includePartialMessages: true,
        canUseTool: this.permit,
        mcpServers: { [PAGE]: this.pageServer() },
      },
    })
    this.read(this.running)
  }

  private async read(running: Query) {
    try {
      for await (const message of running) this.take(message)
    } catch (error) {
      this.emit({ kind: "error", text: error instanceof Error ? error.message : String(error) })
    }
    if (this.running === running) this.running = undefined
    this.idle()
  }

  private idle() {
    this.busy = false
    this.emit({ kind: "state", busy: false })
  }

  private take(message: SDKMessage) {
    if (message.type === "result") {
      if (message.subtype !== "success") this.emit({ kind: "error", text: `The agent stopped: ${message.subtype.replaceAll("_", " ")}` })
      return this.idle()
    }
    if (!("parent_tool_use_id" in message) || message.parent_tool_use_id !== null) return
    if (message.type === "stream_event") {
      const event = message.event
      if (event.type === "content_block_delta" && event.delta.type === "text_delta") this.emit({ kind: "delta", text: event.delta.text })
      return
    }
    if (message.type === "assistant") {
      if (message.error) this.emit({ kind: "error", text: `The agent could not answer: ${message.error.replaceAll("_", " ")}` })
      return eventsOf("assistant", message.message, message.uuid).forEach((event) => this.emit(event))
    }
    if (message.type === "user") {
      const results = eventsOf("user", message.message, randomUUID()).filter((event) => event.kind === "tool_result")
      results.forEach((event) => this.emit(event))
    }
  }

  async send(text: string, shown: string, about?: string) {
    await this.loaded
    this.fresh = false
    this.busy = true
    this.emit({ kind: "user", id: randomUUID(), text: shown, about })
    this.emit({ kind: "state", busy: true })
    this.inbox.push({ type: "user", message: { role: "user", content: text }, parent_tool_use_id: null, origin: { kind: "human" } })
    if (this.running) this.wake?.()
    else await this.start().catch((error) => {
      this.emit({ kind: "error", text: `Could not start the agent: ${error instanceof Error ? error.message : error}` })
      this.idle()
    })
  }

  async interrupt() {
    await this.running?.interrupt()
  }
}

export { Chat }
export type { ChatEvent }
