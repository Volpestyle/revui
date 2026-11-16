# revui on the GPU

The review as a window: the list of your reviews, the summary with its map of the system,
findings beside their diff, your verdicts and comments, the exact text before you publish,
and the review's agent as a chat. One Rust interface, [egui](https://github.com/emilk/egui)
on [wgpu](https://wgpu.rs), runs as a Mac app and, compiled to WebAssembly, in a browser.

    ./bundle.sh                    # the Mac app: target/revui.app and target/revui.dmg
    trunk build --release          # the browser build, which revui web serves
    cargo run -- <bundle dir>      # the app from source, on one review

## How it fits together

- `revui` does the work. The interface asks it for the review (`revui json`) and the list
  (`revui list`), saves your marks through it (`revui state`) and publishes through it
  (`revui publish`), so the terminal and the window read and write the same bundle.
- `src/host.rs` is the only part that differs between the two: the app runs `revui` and
  reads files itself, and the browser build asks the server `revui web` starts.
- `revui web` (`../src/web.rs`) serves `dist/`, runs the handful of `revui` commands the
  page may ask for, and serves a bundle's files. It answers this machine only, and only
  the page it served.

## The agent

One agent per review, the one the terminal viewer uses: the session is kept in the
bundle's `session.json`. In the app each message is one run of the `claude` command your
config names, so it may do what that command is set up to allow, and nothing asks you in
between. It starts in the review's repo: the app asks once for the folder your code is kept
under, and `revui checkout` finds each repo there, or starts the agent in that folder to
find its own way.

The conversation is that Claude Code session and nothing else. The panel reads it from
the transcript Claude Code keeps, so it shows what was said before the window opened, and
rolls the agent's tool calls up into a line you can open: to the calls, then to what each
ran and what came back. The terminal button in its header carries the same session on in
a terminal of yours, as `revui resume <dir>` does from any terminal, and what happens
there shows up in the panel. In a browser there is no process to start: Ask, Verify and Diagram go to an agent
in a pane of the app the page sits in (tmux, Herdr, Orca, Cursor), through `revui agent`.

## Checking and recording it

`REVUI_GPU_KEYS` plays keys with no one at the keyboard, and `REVUI_GPU_SHOT` saves what
the window shows and quits:

    REVUI_GPU_AT=2 REVUI_GPU_KEYS="Tab,J,J,C,text:hello" REVUI_GPU_SHOT=/tmp/shot.png cargo run -- ../examples/demo
