import { spawn } from "node:child_process"
import { existsSync } from "node:fs"
import { fileURLToPath } from "node:url"

const built = ["release", "debug"].map((profile) => fileURLToPath(new URL(`../../target/${profile}/revui`, import.meta.url)))
const bin = process.env.REVUI_BIN ?? built.find(existsSync) ?? "revui"

type Ran = { ok: boolean; out: string; problem: string }

function run(args: string[], input?: string): Promise<Ran> {
  return new Promise((resolve) => {
    const child = spawn(bin, args, { stdio: ["pipe", "pipe", "pipe"] })
    const [out, err]: [Buffer[], Buffer[]] = [[], []]
    child.stdout.on("data", (chunk) => out.push(chunk))
    child.stderr.on("data", (chunk) => err.push(chunk))
    child.on("error", (error) => resolve({ ok: false, out: "", problem: `${bin}: ${error.message}` }))
    child.on("close", (code) => resolve({ ok: code === 0, out: Buffer.concat(out).toString(), problem: Buffer.concat(err).toString().trim() }))
    child.stdin.end(input ?? "")
  })
}

async function revui(args: string[], input?: string): Promise<string> {
  const ran = await run(args, input)
  if (!ran.ok) throw new Error(ran.problem || `revui ${args[0]} failed`)
  return ran.out
}

export { revui, run }
