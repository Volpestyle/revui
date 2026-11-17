# Configuring revui

Everything lives in one JSON file, `~/.config/revui/config.json`. `,` inside
revui opens it in a tab, and `revui doctor` checks it. Every key is optional:
with none, pasting a link reviews it with Claude Code in a tab.

| Key | What it sets | Default |
| --- | --- | --- |
| `claude` | The command that starts Claude Code, available to every command as `$REVUI_CLAUDE`. | `claude` |
| `agents` | The coding agent that reviews, and that `t`, `v` and `i` ask. | Claude Code in a tab |
| `guide` | What to add to the guide revui ships, in your words: a review skill of your team's to follow for the findings, what to look hardest at. | not set |
| `generate`, `generate_modal` | A command of your own that writes guides, in place of the review's agent and the guide revui ships, and whether it runs in a tab. | not set |
| `prepare` | A command that writes a guide with no one watching, for `revui prepare`. | not set |
| `open`, `open_more`, `open_modal` | What `o` runs to open a file. | the line on the web |
| `terminal` | For the window interface, which has no terminal of its own: the command that runs `$REVUI_SCRIPT` in a terminal of yours, when your editor or the agent needs one. | the machine's terminal app |
| `drive` | How agents should drive a browser or app on this machine. | they pick a background tool |
| `accent` | The highlight colour: a name or a hex value such as `"#d77757"`. | `#9a9ef2` |
| `tickets` | Where a ticket id in a review opens, in the window interface: a URL with `{}` where the id goes, such as `"https://linear.app/acme/issue/{}"`. | not set: ticket ids are plain text |
| `audience` | `"product"` for a reader who does not read code: each card leads with what it means for the product, with the technical text under "In detail", and the diff starts hidden (`d` shows it). | not set: the technical text, with the diff |

`REVUI_HOME=<dir>` keeps the config and all data in that one folder instead, for
a second setup or a sandbox.

Commands you configure get their data through environment variables, never by
pasting text into the command line:

| Variable | Holds |
| --- | --- |
| `$REVUI_URL`, `$REVUI_REPO`, `$REVUI_SHA` | the merge request, its project path, its head commit |
| `$REVUI_DIR` | the bundle directory |
| `$REVUI_FILE`, `$REVUI_LINE`, `$REVUI_FILE_URL` | the file and line under the cursor, and its link on the web |
| `$REVUI_HEAD_FILE`, `$REVUI_BASE_FILE` | that file at the head and before the change, fetched read-only |
| `$REVUI_PROMPT`, `$REVUI_ITEM` | what revui would say to an agent, and the finding it is about |
| `$REVUI_PROMPT_URI` | the same prompt, encoded for a URL |
| `$REVUI_SESSION_ID`, `$REVUI_SESSION` | a new agent's id, and whatever its `run` command printed |
| `$REVUI_SOCKET` | a socket path that is stable for the session, for editors that take remote commands |
| `$REVUI_CLAUDE` | the `claude` setting |

## Your reviews

revui opens on a paste box over the reviews you have started, the latest first.
Paste a merge request or pull request link, or run `revui <link>`, and the review
starts. Typing a few letters filters the list. There is nothing to configure: the
list is the bundles under `~/.local/share/revui/`, one per head commit, and a
merge request shows once however many heads it has had.

Opening a review asks the code host whether the head moved. When it did, the
earlier guide opens with an offer to update it for the new commits. The agent
reviews what changed between the two heads and keeps every finding's id, and
your marks come with you: a verdict stays on a finding that reads the same and is
not fixed, what you already posted is not posted again, and your drafts are kept.
A draft on a line of a file that changed since is kept as a comment on the whole
merge request, with the old place named in its text. A finding that was reworded
or fixed is yours to mark again.

`w` asks the code host for the open merge requests and pull requests that request
your review, drafts left out, and adds them to the list as `waiting`. `enter` on
one starts its review. `revui queue` prints the same list.

## Guides ahead of time

`revui prepare` writes the guide for every merge request that is waiting for your
review and has none for its current head, one after another, with no one watching.
`revui prepare <link>` does it for one. Run it from a scheduler and the guides are
there when you sit down.

It runs the `prepare` command of the config, which has to finish without asking
anything. It gets the same variables as `generate`, and `$REVUI_PROMPT` holds what
revui would tell an agent. What it prints goes to `generate.log` in the bundle.
A guide the command leaves half written counts as a failure, and the next run
takes it up again. `revui queue` and `revui prepare` exit 1 when a host could not
be asked, so a scheduler notices a lapsed sign-in.

    "prepare": "claude -p \"$REVUI_PROMPT\" --allowedTools Bash,Read,Write,Edit,Glob,Grep"

There is no default, on purpose. An agent in a tab asks before it runs a command,
and this one cannot, so the command has to say what it may do without asking. It
reads merge requests other people wrote: give it no more than you would let that
text steer.

## Guides from CI

A project can write the guide in its pipeline, so everyone who opens the merge
request reads the same one and nobody waits for a review to start. revui looks for
it whenever a merge request has no guide here: on GitLab the artifact of a job named
`revui-guide` in the merge request's pipeline for the current head (or its newest
pipeline, which is what a merged-results pipeline is), on GitHub an artifact named
`revui-guide` from a workflow run on the head commit. It holds `review.json` and
`assets/`, and `verify.json` with its logs when the pipeline ran the change in a
sandbox. A guide for another head, an unfinished one, or one that does not fit
the diff is ignored.

Whoever can run a pipeline on the merge request's branch can write that artifact,
and that includes the author of the change. So a guide from CI says so at the top
of its summary and on its row under your reviews, and you should read it as the
author's pipeline speaking, not as your own agent. Runs from forks are never used.
Links inside the artifact are not followed, and a guide whose evidence names a file
outside the bundle is ignored.

Marks carry from one head to the next only for findings that read the same, and a
pipeline that starts from nothing each time writes them afresh. For marks to follow
a CI guide, the job has to keep its bundles between runs (cache `REVUI_HOME`), so
the agent updates the earlier guide instead of writing a new one.

[ci.md](ci.md) sets one up: `revui ci gitlab` and `revui ci github` print a job for
each host, which writes the guide with a reviewer that can only read, and keeps
`guide.html` beside it, the same review as a page anyone can open from the merge
request.

## Writing guides

Pasting a link starts the review's agent on it. `g` does the same in a review
that has no guide, and regenerates one that has. The agent is the first entry of
`agents`, in its tab: it is told to run `revui skill guide` and follow it, so no
skill has to be installed and any coding agent can do it. `ctrl-q` shows the
agent, which may ask you to allow a command, and goes back to the review.

The guide is written in stages. The summary and the parts land first, then each
finding as it is found, and the open review picks each one up without moving you
from the card you are on. The top line says what the agent is on until it is done.

The guide the agent follows ships with revui (`revui skill guide`), so every
review gets what it asks for, such as the map of where the change sits. To add
your own way of reviewing to it, set `guide` to what the agent should also do:

    "guide": "For the findings, follow our /team-review skill. Look hardest at migrations."

To replace it, and write guides with something else entirely (a CI job, a
command of your own), set `generate` to a command. What revui's guide gains later
then has to be added to yours by hand. The command gets the merge request in `$REVUI_URL` and the bundle
in `$REVUI_DIR`, and runs in the background unless you add `"generate_modal": true`:

    "generate": "tmux new-window \"claude '/revui-guide $REVUI_URL'\""

Anything can write a guide: `revui format` prints the format, and
`revui check <dir>` validates the result.

## Agents

Each review has one agent: the one that wrote its guide. `t` goes to it, already
told what you are looking at, so you can ask it about the change. `v` asks it to verify the finding
under the cursor (or a section's unproven findings) by running something, and to
record the result in the guide: `confirmed` with observed evidence, or
`unreproduced`. The open review reloads when the agent saves. To verify specific
claims, pick evidence entries first with `space` or a click: the detail pane's
bottom edge shows how many are picked, and `v` sends just those.

Pressing `t` or `v` again goes to the agent that is already running for this
merge request, or picks its conversation up again if it was quit. `T` and `V` let
you choose instead: that agent, or a new one in any of the places you configured.
An agent that is still writing a guide keeps running when you go back to your
reviews; one that is idle is closed, and resumed the next time you ask.

Where an agent opens is the `agents` list in the config. The first entry is the
default, and `T`, `V` and `I` offer the others.

With no `agents` in the config, revui picks from where it is running:

| Running in | Default | Also offered |
| --- | --- | --- |
| a plain terminal | a tab inside revui | |
| tmux | a tmux split, or a tmux window when the terminal is under 200 columns | the other one, a tab inside revui |
| Herdr (the desktop app) | a split pane, or a tab under 200 columns | the other one, a tab inside revui |
| Orca | an Orca terminal tab | an Orca split, a tab inside revui, an Orca chat |
| Cursor | a tab inside revui | Cursor's chat |

A split halves the review's width, so it is only the default when the terminal is
wide enough to leave the review about a hundred columns. Follow-ups go to the pane
the agent first opened in. A chat that starts over on every ask (Cursor's, Orca's)
is offered but never the default.

The tab inside revui runs Claude Code in a full-screen tab over the review. `ctrl-q` goes back to the review while it keeps working, and
`ctrl-q` again, or the tab's number, brings the tabs forward. Keep `ctrl` held and
press `q` again to step through the tabs; let go, and the next `ctrl-q` goes back
to the review. That needs a terminal that reports key releases (`revui probe` says
whether yours does). In one that doesn't, `ctrl-q` only switches between the tabs
and the review: click a tab or press its number to change tabs.

    "agents": [
      { "name": "agent", "modal": true,
        "run": "$REVUI_CLAUDE --session-id \"$REVUI_SESSION_ID\"",
        "resume": "$REVUI_CLAUDE --resume \"$REVUI_SESSION_ID\"" }
    ]

- `run` starts a new agent. revui then types a one-line prompt into its input
  (which guide, and what you are looking at) and leaves it for you to edit and
  send. A `run` that passes `$REVUI_PROMPT` itself sends it straight away instead.
- `modal: true` runs it in a tab over the review. Follow-ups are typed into that
  tab. If you quit the agent, `resume` brings the same conversation back the next
  time, in the directory it started in, even after restarting revui.
  `$REVUI_SESSION_ID` is a UUID revui makes for each new agent.
- Without `modal`, the command must return at once (a split, a tab, a window).
  `send` then delivers follow-ups to it: whatever `run` printed is in
  `$REVUI_SESSION`, so print the pane or session id there. Leave `send` out and
  every press starts a new agent.

A tmux split that is reused:

    { "name": "split",
      "run": "tmux split-window -h -P -F '#{pane_id}' -e \"REVUI_PROMPT=$REVUI_PROMPT\" 'claude \"$REVUI_PROMPT\"'",
      "send": "tmux send-keys -t \"$REVUI_SESSION\" \"$REVUI_PROMPT\" Enter" }

### In your app

Entries for apps that have their own chat, panes or browser. Each was run as written
here, with the paths changed; `revui setup` can write them for you.

**Cursor**, from its terminal. The prompt opens in Cursor's chat through its own link
handler. Cursor asks you to confirm each one and starts a new chat every time, so
there is no `send`. `workspace` is the name of the folder the Cursor window has open.

    "agents": [
      { "name": "Cursor chat",
        "run": "open \"cursor://anysphere.cursor-deeplink/prompt?workspace=shop&text=$(jq -rn --arg p \"$REVUI_PROMPT\" '$p|@uri')\"" }
    ],
    "open": "cursor -d \"${REVUI_BASE_FILE:-/dev/null}\" \"$HOME/code/shop/$REVUI_FILE\""

**Orca**, from a terminal in the workspace of the merge request. The CLI is inside
`Orca.app/Contents/Resources/bin` when it is not on your PATH.

    "agents": [
      { "name": "Orca chat",
        "run": "orca agent launch --agent claude --surface chat --focus --prompt \"$REVUI_PROMPT\"" }
    ],
    "open": "orca file open \"$HOME/code/shop/$REVUI_FILE\" --line \"$REVUI_LINE\" --focus",
    "drive": "Web pages: Orca's embedded browser, in a tab of this workspace (orca tab create --url <url>, then orca snapshot, orca click and orca screenshot)."

**Herdr**, from a pane of the desktop app:

    "agents": [
      { "name": "Herdr split",
        "run": "herdr-browser do terminal.open --place right --argv \"$(jq -cn --arg p \"$REVUI_PROMPT\" '[\"claude\",$p]')\"",
        "send": "hc send \"$(printf %s \"$REVUI_SESSION\" | jq -r .terminal)\" \"$REVUI_PROMPT\"" }
    ],
    "drive": "Web pages: Herdr's in-app browser, in a pane beside you (herdr-browser do browser.open --url <url> --place right)."

**Ghostty**, through its AppleScript. `run` is a script that splits the terminal revui
is in and prints the new split's id, and `send` types into that split:

    #!/bin/sh
    # ghostty-split.sh
    exec osascript <<'AS'
    set thePrompt to system attribute "REVUI_PROMPT"
    tell application "Ghostty"
      set cfg to new surface configuration
      set command of cfg to "/bin/zsh -lc 'claude \"$REVUI_PROMPT\"'"
      set environment variables of cfg to {"REVUI_PROMPT=" & thePrompt}
      set here to focused terminal of selected tab of front window
      return id of (split here direction right with configuration cfg)
    end tell
    AS

    #!/bin/sh
    # ghostty-send.sh
    exec osascript <<'AS'
    set thePrompt to system attribute "REVUI_PROMPT"
    set theSplit to system attribute "REVUI_SESSION"
    tell application "Ghostty"
      set target to first terminal whose id is theSplit
      input text thePrompt to target
      send key "enter" to target
    end tell
    AS

    "agents": [
      { "name": "Ghostty split", "run": "~/bin/ghostty-split.sh", "send": "~/bin/ghostty-send.sh" }
    ]

Agents learn how to read and update a bundle from `revui skill`, which the prompt
tells them to run. `revui skill install` also installs it as a Claude Code skill,
so any session can work on a bundle when you ask.

## Opening files

`enter` opens the file under the cursor. The default opens it on the web at that line
(`$REVUI_FILE_URL`). Set `open` to use an editor on a local clone. It gets
`$REVUI_FILE`, `$REVUI_LINE`, `$REVUI_REPO` (the project path) and `$REVUI_SHA`:

    "open": "nvim +$REVUI_LINE ~/src/$REVUI_REPO/\"$REVUI_FILE\""
    "open": "code -g ~/src/$REVUI_REPO/\"$REVUI_FILE\":$REVUI_LINE"

A local clone shows the branch it has checked out, which may not be the merge
request's head. `$REVUI_HEAD_FILE` is the file as it is at the head and
`$REVUI_BASE_FILE` the same file before the merge request: revui fetches them
into the bundle, read-only, the first time a command asks. Open both in your
editor's diff mode to see the change in context.

In the window interface a click on a block of the map opens its changed file with the
same command. An editor that needs a terminal (`open_modal`) is started in one of
yours: by default the machine's terminal app, or whatever `terminal` runs, such as a
new tab of the app you work in:

    "terminal": "herdr-browser do tab.new --name revui --argv \"$(jq -cn --arg s \"$REVUI_SCRIPT\" '[\"sh\",$s]')\""

With `"open_modal": true` the command runs in a terminal drawn over the review
instead of taking the screen. `ctrl-q` hides it and `enter` shows it again. To
collect several files in one editor, add `open_more`: it runs when the editor is
already open, with `$REVUI_SOCKET` naming a socket path that is stable for the
session. For Neovim, which then shows each file as a tab:

    "open_modal": true,
    "open": "nvim --listen \"$REVUI_SOCKET\" +$REVUI_LINE \"$REVUI_HEAD_FILE\"",
    "open_more": "nvim --server \"$REVUI_SOCKET\" --remote-tab \"$REVUI_HEAD_FILE\""

## How agents drive the product

`v` and `i` ask the agent to reproduce something, off your screen. Two things
decide how.

**Where it runs** is per repo, and the agent asks once: the first time a check
needs the product running, it asks where the change runs (a local stack, or a
preview deploy of the branch) and where today's behaviour is read from (the base
commit locally, or staging or production, read only), with what it found in the
repo and on the merge request as suggestions. The answer is kept under
`## Evidence` in that repo's notes, `~/.local/share/revui/notes/<repo>.md`, which
you can edit. Saying otherwise in a request ("check this one on staging") wins for
that request. A deployed environment only has merged code, so it is never used to
show what an unmerged change does.

**What drives it** is per machine. Say it in `"drive"`: one or two sentences naming
what to use here (your terminal app's browser, Playwright, `agent-desktop`, a
local-stack skill). Agents read both with `revui drive`. Unset, they pick a
background tool they can find and ask before anything that would take the screen.

The how is a second bundled skill, `revui-drive` (`revui skill drive` prints it,
`revui skill install` installs it with the other). It is generic: it finds the
local stack from the repo, picks a driving tool from what `revui drive` reports on
the machine, and adds what it learns to the same notes file, so the start command
and sign-in are worked out once. A project's own skill for
running its product still wins when there is one.

A third bundled skill, `revui-guide`, is what `g` runs by default: it reviews the
merge request or pull request and writes the guide. Point `generate` at your own
review skill when you have one that follows `revui format`.

## Images

Evidence screenshots render inline in terminals with Kitty, Sixel or iTerm2
graphics, and as block characters elsewhere. `REVUI_IMAGES=halfblocks` forces
block characters.
