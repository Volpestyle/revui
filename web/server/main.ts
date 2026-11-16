import { createReadStream, existsSync, statSync, watch } from "node:fs"
import { createServer } from "node:http"
import type { IncomingMessage, ServerResponse } from "node:http"
import { extname, join, resolve, sep } from "node:path"
import { fileURLToPath } from "node:url"
import { Chat } from "./chat.ts"
import type { ChatEvent } from "./chat.ts"
import { revui, run } from "./revui.ts"

const TYPES: Record<string, string> = {
  ".png": "image/png",
  ".jpg": "image/jpeg",
  ".jpeg": "image/jpeg",
  ".gif": "image/gif",
  ".webp": "image/webp",
  ".svg": "image/svg+xml",
  ".mp4": "video/mp4",
  ".webm": "video/webm",
  ".mov": "video/quicktime",
  ".html": "text/html",
  ".js": "text/javascript",
  ".css": "text/css",
  ".woff2": "font/woff2",
}
const MODES = ["ask", "verify", "artifact", "guide"]

const named = process.argv.slice(2).find((arg) => !arg.startsWith("--"))
const agent = !process.argv.includes("--no-agent")
if (!named) {
  console.error("usage: node server/main.ts <bundle directory | merge request link> [--no-agent]")
  process.exit(2)
}
const dir = resolve(/^https?:\/\//.test(named) ? (await revui(["fetch", named])).trim() : named)
const port = Number(process.env.PORT ?? 4319)
const hosts = new Set([`127.0.0.1:${port}`, `localhost:${port}`])
const root = resolve(process.env.REVUI_WEB ?? fileURLToPath(new URL("..", import.meta.url)))
const production = process.env.NODE_ENV !== "development" && existsSync(join(root, "dist/index.html"))

const review = async () => ({ ...JSON.parse(await revui(["json", dir])), agent })
const chat = agent ? new Chat(dir, (await review()).claude) : undefined
const streams = new Set<(event: ChatEvent | { kind: "review" }) => void>()

let settle: NodeJS.Timeout | undefined
watch(dir, (_, file) => {
  if (file !== "review.json" && file !== "state.json" && file !== "threads.json") return
  clearTimeout(settle)
  settle = setTimeout(() => streams.forEach((send) => send({ kind: "review" })), 150)
})

const vite = production ? undefined : await (await import("vite")).createServer({ root, server: { middlewareMode: true }, appType: "spa" })

function trusted(request: IncomingMessage) {
  const host = request.headers.host ?? ""
  if (!hosts.has(host)) return false
  return request.method === "GET" || (request.headers.origin === `http://${host}` && request.headers["content-type"] === "application/json")
}

async function body(request: IncomingMessage) {
  const chunks: Buffer[] = []
  for await (const chunk of request) chunks.push(chunk)
  return JSON.parse(Buffer.concat(chunks).toString() || "{}")
}

function json(response: ServerResponse, value: string | object, status = 200) {
  response.writeHead(status, { "content-type": "application/json" })
  response.end(typeof value === "string" ? value : JSON.stringify(value))
}

function file(response: ServerResponse, path: string, inside: string) {
  const full = resolve(inside, path)
  if (!full.startsWith(inside + sep) || !existsSync(full) || !statSync(full).isFile()) return false
  response.writeHead(200, { "content-type": TYPES[extname(full).toLowerCase()] ?? "text/plain; charset=utf-8", "x-content-type-options": "nosniff" })
  createReadStream(full).pipe(response)
  return true
}

async function events(request: IncomingMessage, response: ServerResponse) {
  response.writeHead(200, { "content-type": "text/event-stream", "cache-control": "no-cache" })
  const send = (event: object) => void response.write(`data: ${JSON.stringify(event)}\n\n`)
  streams.add(send)
  const leave = (await chat?.subscribe(send, true)) ?? (() => {})
  request.on("close", () => {
    streams.delete(send)
    leave()
  })
}

async function say(chat: Chat, sent: { text?: string; mode?: string; on?: string }) {
  const mode = MODES.includes(sent.mode ?? "") ? sent.mode! : "ask"
  const on = /^[\w.-]+$/.test(sent.on ?? "") ? [sent.on!] : []
  const prompt = (await revui(["prompt", dir, mode, ...on, ...(chat.fresh ? ["--fresh"] : [])])).replace(/\n$/, "")
  const text = (sent.text ?? "").trim()
  if (mode !== "ask") return chat.send(prompt, prompt)
  if (text) return chat.send(prompt + text, text, on[0])
}

async function api(request: IncomingMessage, response: ServerResponse, path: string) {
  const method = request.method
  if (path === "/api/events" && method === "GET") return events(request, response)
  if (path === "/api/review" && method === "GET") return json(response, await review())
  if (path === "/api/state" && method === "PUT") {
    await revui(["state", dir], JSON.stringify(await body(request)))
    return json(response, await review())
  }
  if (path === "/api/threads" && method === "POST") {
    await revui(["threads", dir])
    return json(response, await review())
  }
  if (path === "/api/publish" && method === "POST") {
    const { choice, text } = await body(request)
    const ran = await run(["publish", dir, String(Number(choice) + 1)], String(text))
    return json(response, { ok: ran.ok, problem: ran.problem })
  }
  if (path === "/api/agent" && method === "POST") {
    const { mode, on } = await body(request)
    const ran = await run(["agent", dir, MODES.includes(mode) ? mode : "ask", ...(/^[\w.-]+$/.test(on ?? "") ? [on] : [])])
    return json(response, { ok: ran.ok, note: ran.out.trim(), problem: ran.problem })
  }
  if (!chat) return json(response, { problem: "not found" }, 404)
  if (path === "/api/chat" && method === "POST") {
    await say(chat, await body(request))
    return json(response, {})
  }
  if (path === "/api/chat/answer" && method === "POST") {
    const { id, answer } = await body(request)
    chat.answer(String(id), answer)
    return json(response, {})
  }
  if (path === "/api/chat/interrupt" && method === "POST") {
    await chat.interrupt()
    return json(response, {})
  }
  json(response, { problem: "not found" }, 404)
}

const server = createServer(async (request, response) => {
  const path = decodeURIComponent(new URL(request.url ?? "/", "http://localhost").pathname)
  if (!trusted(request)) return json(response, { problem: "forbidden" }, 403)
  try {
    if (path.startsWith("/api/")) return await api(request, response, path)
    if (path.startsWith("/bundle/")) return file(response, path.slice("/bundle/".length), dir) || json(response, { problem: "not found" }, 404)
    if (vite) return vite.middlewares(request, response)
    if (!file(response, path.slice(1), join(root, "dist")) && !file(response, "index.html", join(root, "dist"))) json(response, { problem: "not found" }, 404)
  } catch (error) {
    json(response, { problem: error instanceof Error ? error.message : String(error) }, 500)
  }
})

server.listen(port, "127.0.0.1", () => console.log(`revui: http://127.0.0.1:${port}  (${dir})`))
