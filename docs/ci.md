# Guides from your pipeline

A job in the merge request's pipeline writes the guided review, so it is there before
anyone opens the change:

- **In revui.** Whoever opens the merge request in revui gets the pipeline's guide
  instead of waiting for a review to start ([config.md](config.md), "Guides from CI",
  says how revui finds it and how far it trusts it).
- **As a page.** The job keeps `guide.html`, the same review as one file that opens in
  any browser with no revui, linked from the merge request.

## Set it up

    revui ci gitlab     # prints a job: add it to .gitlab-ci.yml
    revui ci github     # prints a workflow: save it as .github/workflows/revui-guide.yml

Then give the job a key for the reviewer: `ANTHROPIC_API_KEY`, as a masked CI/CD
variable on GitLab or a secret on GitHub. That is all it needs. Keep the job's name,
`revui-guide`: it is what revui looks for.

The job installs revui and Claude Code, runs `revui ci script`, and keeps the folder
`revui-guide/` as its artifact:

    review.json, mr.patch, assets/   the bundle revui reads
    guide.html                       the page
    guide-metric.json                what the run took: model, turns, seconds, cost
    session-*.jsonl                  everything the reviewer did, one event a line
    verify.json, <step>.log          what a sandbox run did, when there was one

A guide is advice. The job is allowed to fail and never blocks the merge request.

## What the reviewer can do

It runs on text other people wrote, with your key, so it is given little:

- **It can only read.** Claude Code gets Read, Grep and Glob over the checkout. No
  shell and no way to write a file: it prints the guide, and the script checks what it
  printed against the diff (`revui check`) and writes the files. An answer that does
  not fit gets one repair pass.
- **No token for your host.** The bundle is built from the checkout and the variables
  the host gives the job, so nothing is asked of GitLab's or GitHub's API.
- **No settings from the checkout.** A `.claude/settings.json` in the merge request
  could carry a hook that runs commands in your job, so none are loaded.
- **Rules come from the target branch.** Your review rules, system map and sandbox
  notes are read from the branch the change merges into, so a merge request cannot
  rewrite the rules it is reviewed by.
- **Not for forks.** The job spends your key on whatever the branch holds. Both jobs
  as printed run only for branches of the project itself.

The whole session is kept in the artifact (`session-guide.jsonl`), so what the
reviewer read and ran does not depend on what it chose to report.

## Make it yours

All of these are optional. The files are read from the target branch.

| What | How |
| --- | --- |
| Your own review rules | `REVIEW.md` at the root of the repo (`GUIDE_RULES` names another file). They add to the reviewer's own, and win where they are stricter. |
| A map of your system | `docs/system-map.mmd`, a Mermaid flowchart of the components and who calls whom (`GUIDE_MAP` names another file). Each guide's map is cut from it. [FORMAT.md](../FORMAT.md) says what it holds. |
| Tickets | `GUIDE_MCP`, a Claude Code MCP config as JSON, and `GUIDE_MCP_TOOLS`, the tools of it the reviewer may call. Name only tools that read. |
| The model | `GUIDE_MODEL` (default `opus`), `GUIDE_EFFORT`, and `GUIDE_TIMEOUT` in seconds for each call (default 900). |

Tickets from Linear, for example, with `LINEAR_API_KEY` set as a masked variable:

    GUIDE_MCP: '{"mcpServers":{"linear":{"type":"http","url":"https://mcp.linear.app/mcp","headers":{"Authorization":"Bearer ${LINEAR_API_KEY}"}}}}'
    GUIDE_MCP_TOOLS: mcp__linear__get_issue,mcp__linear__list_comments,mcp__linear__get_project

`revui ci prompt` prints what the reviewer is told, and `revui ci script` the script
itself.

## Have it test the change

A reviewer that only reads marks everything as reasoned from the code. If your job can
start the change, the reviewer can test it: reproduce a finding before reporting it,
and prove the behaviour the change claims. The guide then carries a verification card
with every step and check, each opening to its output.

1. **Start the change and record it.** `revui ci verify` prints a small recorder to
   source in your job's script. It writes `verify.json` and a log for each step:

       source <(revui ci verify)
       step install npm ci
       step start   ./scripts/start-local.sh
       check 'GET /health' "$(curl -s -o /dev/null -w '%{http_code}' localhost:3000/health)"
       verified http://localhost:3000 || true

   A step that fails is recorded and the later ones still run. `verified` returns 1
   when anything failed.

2. **Say what is running.** `ci/sandbox-notes.md` on the target branch
   (`GUIDE_SANDBOX_NOTES` names another file): the addresses, how to sign in, how to
   query the database, and what is *not* running. The reviewer treats anything the
   note does not name as absent, and lists what it could not exercise.

3. **Hand it to the reviewer.** Set `GUIDE_SANDBOX` to the recorder's folder before the
   script runs:

       GUIDE_SANDBOX=$PWD/sandbox bash /tmp/guide.sh

This gives the reviewer a shell on the job's machine, so it is a different bargain from
the read-only run: use a runner you would let the merge request's own tests run on,
with a stack that is thrown away afterwards, and nothing in the job's environment that
reaches a deployed one. `revui ci sandbox` prints the rules the reviewer is given.

## What it costs

One model run per pipeline, two when the first answer needs repair. A small change
takes well under a minute; a few hundred changed lines with tickets to read, a few
minutes. The page's stats sheet (press `S`) shows what each run took. To spend only
when someone asks, make the job manual (a comment in the GitLab job shows where).
