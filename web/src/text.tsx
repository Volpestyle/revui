import type { ReactNode } from "react"

const URL = /(https?:\/\/[^\s<>)\]]+)/g

function linked(text: string): ReactNode[] {
  return text.split(URL).map((part, n) => (n % 2 ? <a key={n} href={part} target="_blank" rel="noreferrer">{part}</a> : part))
}

function Prose({ text }: { text: string }) {
  const blocks = text.trim().split(/\n{2,}|\n(?=# )/)
  return (
    <div className="prose">
      {blocks.map((block, n) => {
        const lines = block.split("\n")
        const heading = lines[0].startsWith("# ") ? lines.shift()!.slice(2) : undefined
        return (
          <div key={n}>
            {heading && <h4>{heading}</h4>}
            {lines.length > 0 && <p>{linked(lines.join("\n"))}</p>}
          </div>
        )
      })}
    </div>
  )
}

function inline(text: string): ReactNode[] {
  return text.split(/(`[^`\n]+`|\*\*[^*\n]+\*\*)/g).map((part, n) => {
    if (part.startsWith("`") && part.length > 2) return <code key={n}>{part.slice(1, -1)}</code>
    if (part.startsWith("**") && part.length > 4) return <strong key={n}>{part.slice(2, -2)}</strong>
    return <span key={n}>{linked(part)}</span>
  })
}

function Reply({ text }: { text: string }) {
  return (
    <div className="prose reply">
      {text.split(/```[\w-]*\n?/).map((part, n) =>
        n % 2 ? <pre key={n}>{part.replace(/\n$/, "")}</pre> : part.trim() && part.trim().split(/\n{2,}/).map((block, m) => <p key={`${n}-${m}`}>{inline(block)}</p>),
      )}
    </div>
  )
}

export { Prose, Reply }
