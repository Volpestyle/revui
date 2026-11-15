# revui in a browser

The same review as the terminal viewer, drawn as a page: the cards, the diff, your
verdicts and comments, the exact text before you publish, and the review's agent as a
chat beside it.

    cargo install --path .                    # from this checkout, once
    cd web && pnpm install && pnpm build      # once, and after pulling
    revui web <merge request link | bundle directory>

Run `revui web` from the checkout of the code under review, as you would `revui`: the
agent starts there. It prints a local address to open. `PORT` moves it off 4319.

`revui web --no-agent <...>` leaves the agent's chat out: for a page opened inside Orca,
Cursor or Herdr, where your agent already has a pane beside it. Ask, Verify and Diagram in
the header (and `t`) then go to an agent in a pane of that app, the one `revui` itself
would open there: started on the first press, sent to after that (`revui agent`).

As in the terminal, the card fills the page: `b` (or Contents in the header) lists every
card to jump to one, and `B` keeps that list beside the card.

## How it fits together

- `revui` does the work. The page asks it for the review (`revui json`), saves your
  marks through it (`revui state`), and publishes through it (`revui publish`), so the
  terminal and the page read and write the same bundle.
- `server/` is a small Node program on 127.0.0.1. It serves the page, passes those calls
  on, and runs the review's agent with the Claude Agent SDK, through the `claude` command
  your revui config names.
- `src/` is the page, built from [UI SDK](https://github.com/vercel-labs/ui) parts.

## The agent

One agent per review, the one the terminal viewer uses: the page keeps its session in the
bundle's `session.json`, so a conversation started here is picked up by `revui` in a tab,
and the other way round. Open it in one place at a time: resumed twice at once, a session
forks.

Its window opens with the Agent button in the header, or cmd-j (ctrl-j), or `t`, which
also puts the cursor in it. From its title bar it detaches from the side into a window
you drag by the title and size from its left and top edges, minimizes to a bar at the
bottom right that shows when the agent is working, and docks back.

The agent can read and operate the page (go to a card, draft a comment for you) through
UI SDK's agent bridge. Publishing is marked as needing you: when the agent presses it,
the page asks, and nothing is posted unless you allow it. The chat itself, and the
prompts that ask your permission for a command, are hidden from the bridge.

## Develop

    pnpm dev ../examples/demo    # the page reloads as you edit
    pnpm typecheck
    pnpm test                    # needs target/debug/revui: cargo build

`pnpm build` writes `dist/`, which `revui web` serves. Without it, the page is served
from source, as `pnpm dev` does.
