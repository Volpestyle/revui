import assert from "node:assert/strict"
import { spawn } from "node:child_process"
import { request } from "node:http"
import { test } from "node:test"
import { fileURLToPath } from "node:url"

const port = 4391
const demo = fileURLToPath(new URL("../../examples/demo", import.meta.url))

function ask(path: string, method: string, headers: Record<string, string>) {
  return new Promise<number>((resolve, reject) => {
    const sent = request({ host: "127.0.0.1", port, path, method, headers }, (response) => {
      response.resume()
      resolve(response.statusCode ?? 0)
    })
    sent.on("error", reject)
    sent.end(method === "GET" ? undefined : "{}")
  })
}

test("--no-agent serves the review with no agent to talk to", async () => {
  const server = spawn("node", [fileURLToPath(new URL("main.ts", import.meta.url)), demo, "--no-agent"], { env: { ...process.env, PORT: String(port) }, stdio: ["ignore", "pipe", "inherit"] })
  try {
    await new Promise((resolve) => server.stdout.once("data", resolve))
    const host = `127.0.0.1:${port}`
    assert.equal((await (await fetch(`http://${host}/api/review`)).json()).agent, false)
    assert.equal(await ask("/api/chat", "POST", { host, origin: `http://${host}`, "content-type": "application/json" }), 404)
  } finally {
    server.kill()
  }
})

test("only this page, on this machine, can read the review or change anything", async () => {
  const server = spawn("node", [fileURLToPath(new URL("main.ts", import.meta.url)), demo], { env: { ...process.env, PORT: String(port) }, stdio: ["ignore", "pipe", "inherit"] })
  try {
    await new Promise((resolve) => server.stdout.once("data", resolve))
    const host = `127.0.0.1:${port}`
    const json = { "content-type": "application/json" }
    assert.equal(await ask("/api/review", "GET", { host }), 200)
    assert.equal(await ask("/api/review", "GET", { host: "evil.example" }), 403, "a page that rebinds its name to this machine is refused")
    assert.equal(await ask("/api/publish", "POST", { host, ...json }), 403, "a request with no origin is refused")
    assert.equal(await ask("/api/publish", "POST", { host, ...json, origin: "https://evil.example" }), 403, "another site is refused")
    assert.equal(await ask("/api/publish", "POST", { host, origin: `http://${host}`, "content-type": "text/plain" }), 403, "a form post is refused")
    assert.equal(await ask("/bundle/..%2f..%2f..%2fetc%2fpasswd", "GET", { host }), 404, "nothing outside the bundle is served")
  } finally {
    server.kill()
  }
})
