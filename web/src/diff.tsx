import hljs from "highlight.js/lib/common"
import { useEffect, useMemo, useRef } from "react"
import type { Data } from "./types.ts"

type Line = { file: string; line: number; old: boolean }

function escape(text: string) {
  return text.replace(/[&<>]/g, (char) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;" })[char]!)
}

function paint(path: string, code: string) {
  const language = path.split(".").pop() ?? ""
  return hljs.getLanguage(language) ? hljs.highlight(code, { language, ignoreIllegals: true }).value : escape(code)
}

function rowsOf(data: Data, group: number) {
  return data.groups[group].map(([fi, hi], n, all) => {
    const file = data.files[fi]
    const hunk = file.hunks[hi]
    let [oldLine, newLine] = [hunk.old_start, hunk.new_start]
    const lines = hunk.lines.map((text) => {
      const sign = text[0] ?? " "
      const at: Line = { file: file.path, line: sign === "-" ? oldLine : newLine, old: sign === "-" }
      if (sign === "-") oldLine += 1
      else if (sign === "+") newLine += 1
      else if (sign !== "\\") [oldLine, newLine] = [oldLine + 1, newLine + 1]
      return { sign, at, html: paint(file.path, text.slice(1)) }
    })
    return { key: `${fi}-${hi}`, path: n === 0 || all[n - 1][0] !== fi ? file.path : undefined, header: hunk.header, lines }
  })
}

const same = (a: Line | undefined, b: Line) => a !== undefined && a.file === b.file && a.line === b.line && a.old === b.old

function Diff({ data, group, anchor, picked, onPick }: { data: Data; group: number; anchor?: Line; picked?: Line; onPick: (line: Line) => void }) {
  const hunks = useMemo(() => rowsOf(data, group), [data.files, data.groups, group])
  const anchored = useRef<HTMLDivElement>(null)
  useEffect(() => void anchored.current?.scrollIntoView({ block: "center" }), [anchor?.file, anchor?.line, group])
  const noted = (at: Line) => data.state.notes.some((note) => note.file === at.file && note.old === at.old && note.line !== null && at.line >= (note.start ?? note.line) && at.line <= note.line)
  const said = (at: Line) => data.threads.some((thread) => same({ file: thread.file ?? "", line: thread.line ?? 0, old: thread.old }, at))
  return (
    <div className="diff">
      {hunks.map((hunk) => (
        <section key={hunk.key}>
          {hunk.path && <h3>{hunk.path}</h3>}
          <div className="hunk-header">{hunk.header}</div>
          {hunk.lines.map(({ sign, at, html }, n) => (
            <div key={n} ref={same(anchor, at) && sign !== "\\" ? anchored : undefined} className="line" data-sign={sign} data-file={sign === "\\" ? undefined : at.file} data-line={sign === "\\" ? undefined : at.line} data-old={at.old} data-anchor={same(anchor, at) || undefined} data-picked={same(picked, at) || undefined}>
              <button type="button" className="gutter" aria-label={`Comment on ${at.file}:${at.line}`} disabled={sign === "\\"} onClick={() => onPick(at)}>
                {sign === "\\" ? "" : at.line}
              </button>
              <span className="mark" data-mine={noted(at) || undefined} data-theirs={said(at) || undefined}>{sign}</span>
              <code dangerouslySetInnerHTML={{ __html: html || " " }} />
            </div>
          ))}
        </section>
      ))}
    </div>
  )
}

export { Diff }
export type { Line }
