import { useRef, useState } from "react"
import type { PointerEvent as Press, ReactNode } from "react"

const KEY = "revui-agent-size"
const SIZE = { w: 420, h: 640 }
const MIN = { w: 340, h: 360 }
const EDGE = 16
const HANDLES = ["x", "y", "xy"] as const

function saved() {
  try {
    const size = JSON.parse(localStorage.getItem(KEY) ?? "")
    return { w: Number(size.w) || SIZE.w, h: Number(size.h) || SIZE.h }
  } catch {
    return SIZE
  }
}

const within = (value: number, least: number, most: number) => Math.round(Math.min(Math.max(most, least), Math.max(least, value)))

function follow(event: Press, move: (dx: number, dy: number) => void, done?: () => void) {
  event.preventDefault()
  const start = { x: event.clientX, y: event.clientY }
  const onMove = (now: PointerEvent) => move(now.clientX - start.x, now.clientY - start.y)
  const onUp = () => {
    window.removeEventListener("pointermove", onMove)
    window.removeEventListener("pointerup", onUp)
    done?.()
  }
  window.addEventListener("pointermove", onMove)
  window.addEventListener("pointerup", onUp)
}

/** A window over the page, kept to its bottom right corner: dragged by whatever `children` gives `grab` to, sized from its left and top edges. */
function Float({ label, children }: { label: string; children: (grab: (event: Press) => void) => ReactNode }) {
  const [size, setSize] = useState(saved)
  const [away, setAway] = useState({ x: 0, y: 0 })
  const latest = useRef(size)
  latest.current = size

  const grab = (event: Press) => {
    if ((event.target as HTMLElement).closest("button")) return
    const from = away
    follow(event, (dx, dy) => setAway({ x: within(from.x - dx, 0, innerWidth - size.w - EDGE * 2), y: within(from.y - dy, 0, innerHeight - size.h - EDGE * 2) }))
  }

  const resize = (axes: string) => (event: Press) => {
    const from = size
    const most = { w: innerWidth - away.x - EDGE * 2, h: innerHeight - away.y - EDGE * 2 }
    follow(
      event,
      (dx, dy) => setSize({ w: axes.includes("x") ? within(from.w - dx, MIN.w, most.w) : from.w, h: axes.includes("y") ? within(from.h - dy, MIN.h, most.h) : from.h }),
      () => localStorage.setItem(KEY, JSON.stringify(latest.current)),
    )
  }

  return (
    <section className="float" aria-label={label} style={{ right: EDGE + away.x, bottom: EDGE + away.y, width: size.w, height: size.h }}>
      {HANDLES.map((axes) => (
        <div
          key={axes}
          className="edge"
          data-axes={axes}
          aria-hidden
          onPointerDown={resize(axes)}
          onDoubleClick={() => {
            setSize(SIZE)
            localStorage.removeItem(KEY)
          }}
        />
      ))}
      {children(grab)}
    </section>
  )
}

export { Float }
