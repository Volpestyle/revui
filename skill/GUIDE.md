---
name: revui-guide
description: >-
  Review a GitLab merge request or GitHub pull request and write the review as a guided-review bundle for the `revui` terminal viewer: the change split into ordered parts, each with why it exists and the findings for its code. Use when asked for a guide or a guided review of a merge request or pull request URL, or with /revui-guide <url>. Read-only: it never posts, pushes or approves.
---

# Writing a guided review

You are given the URL of a merge request or pull request. The result is a bundle the person reads in `revui` while you write it: a summary first, every changed hunk placed in a part, then one finding at a time, each attached to the code it is about.

Read-only, always: never post a comment, push, approve or change the branch.

## 1. Get the change

1. `dir="$(revui fetch <url>)"` creates or finds the bundle for the current head: `mr.patch` and a `review.json` with no sections. Never edit `mr.patch`.
2. Read the description, the linked ticket if there is one, and the conversation already on it (`glab mr view --comments`, or `gh pr view --json title,body,comments,reviews` plus `gh api repos/<owner>/<repo>/pulls/<number>/comments` for comments on lines).
3. Get the code at the head, in a scratch worktree or a fresh clone, never in the person's checkout. Read the diff end to end: enough to see what the change is made of.

If `$dir/previous` exists, an earlier head already has a guide: read "A merge request that moved on" in `revui format` and update that guide instead of starting over. If `review.json` already has sections for this head, the person asked to regenerate: review again from scratch and write it fresh.

## 2. Write the outline first

The person is waiting with the viewer open, so get them something to read before you go deep. Run `revui format` and follow it: "Writing in stages" says how the file is written. `revui hunks "$dir"` lists every file's hunks by index.

Write the outline to `review.json` in one go, as soon as you understand the shape of the change:

- **Summary**: one short paragraph. What the ticket asks and what the change does, including anything beyond the ask. Then one line naming the part you expect to matter most, so the reader knows where the risk is before the findings arrive. That line is a placeholder: the finished summary replaces it. Also write `plain`: the same in a sentence or two for someone who does not read code, as what changes for the people using the product.
- **Parts** (`sections`): group hunks that belong together, the core of the change first, supporting code next, low-signal churn (renames, generated files, formatting) last. Each part gets one paragraph on why it exists. Place every hunk: split a file across parts by hunk when its hunks serve different purposes. This order is the order you review in and the order the person reads in, so do not change it later.
- **`working`**: `Reviewing part 1 of N: <title>`.

### The map

Right after the outline is in the file, draw where this change sits, so the person sees the system before the findings. Set `working` to `Drawing the map` while you do. Skip it, and say nothing about it, for a change that stays inside one component and moves no call between components: a copy change, a rename, a fix inside one function.

1. `revui map "$dir"` prints where this repo's system map is kept and what it holds. It is Mermaid: the components (apps, services, stores, queues, the outside systems they talk to) and who calls whom, as the target branch has it. Ten to twenty nodes, and nothing as small as a file or a function.
2. None yet: draw it from the code and save it at that path. Every later review of this repo starts from it, so take the care once. One there already: check only the part this change touches against the code, and correct what is wrong.
3. Never put this change into the system map. It describes the target branch, and this change may not merge.
4. Copy the components this change touches, and their direct neighbours, into `assets/map.mmd`, and mark what the change does to them:

       flowchart LR
         web[Customer web] --> api[Orders API]
         api --> db[(Orders table)]
         api -. new .-> queue[[Retry queue]]
         classDef changed stroke:#e8a33d,stroke-width:3px
         classDef added stroke:#3fb950,stroke-width:3px
         classDef removed stroke:#f85149,stroke-dasharray:4
         class api changed
         class queue added

5. `revui diagram assets/map.mmd assets/map.png`, then set `map` in `review.json`: `provenance` `inferred`, `path` `assets/map.png`, a `caption` of one sentence saying what moves, and as `text` what changes at each marked place, numbered, in the order the parts are read.

If the diagram cannot be rendered here, leave `map` out and go on: the review does not wait for it.

## 3. Review part by part, one finding at a time

Take the parts in order. For each, read its changed files whole, not only their hunks, and follow the callers of what changed. Look for problems this change introduces: does it do what the ticket asks, security and data integrity, runtime behaviour and edge cases, error handling, then consistency with the code around it and needless complexity.

- Every finding needs evidence from the code at the head: the file and line, and the path that makes it go wrong. Trace a realistic input before calling something a bug.
- A risk you cannot settle by reading (concurrency, ordering, another system's behaviour) is a question, with what you checked and why it stays open.
- Know how this repo releases before faulting what the pipeline did not cover: read its CI config, when it has one. "This was not tested in staging" is not a finding when staging only ever sees merged code.
- Note what the change does well where it is worth knowing, and any choice it made that the ticket did not ask for.
- Leave out taste, and anything that was already there before this change.

**Write each finding to `review.json` the moment you have it**, then go on. Do not hold findings back to sort or polish them: the person reads the first while you look for the second. When you move to the next part, update `working`.

- **Ids**: by severity, numbered as they are found: `B1`, `F1`, `S1`, `Q1`, with `severity` set to exactly `blocker`, `fix`, `suggestion` or `question` (the format allows any text; guides written with this skill use these four). Good calls are `G1`, `G2`, with no `severity`. An id never changes once it is in the file.
- **Items**: each under the part whose code it is about, with `at` pointing at the line to fix. Give each a `plain`: one or two sentences on what a person using the product would see, with no file or function names ("A customer can be charged twice for one checkout"). When a finding changes nothing they could see, say that ("Nothing a customer would notice: it makes the next change here harder"). In the body, put what breaks, the path to it, and what to do instead, each under its own `# ` heading. A `Q` about a choice the change made where the ticket was silent is a `decision`.
- **Evidence**: only what you have. Something you worked out by reading is `inferred`. Output of something you ran is `observed`, with the command as `repro` and whole lines of output. Never mark as `observed` what was not run.

### When to capture evidence before the person asks

Most findings go out `inferred`: the person can have any of them verified with one key. Run something yourself, before writing the finding, only when all three hold:

- the finding claims behaviour (what a user or a caller would see), not structure;
- whether it is a real problem hinges on that behaviour, and reading leaves real doubt;
- the check is cheap here: a test, a script, a request, a query, or a server the README starts with one command and no setup. Minutes, not a stack you have to work out how to start.

For a blocker, the first two are enough reason to try: when the run is that cheap, do it rather than send a blocker out unverified. A blocker you did not get to this way is verified at the end, in step 4, so the findings after it are not held up.

Where a run happens is the person's choice, per repo. `revui drive "$dir"` prints it under "Where evidence comes from". If nothing is recorded there yet and a run needs more than the code itself (a running stack, a deployed environment), write the finding as `inferred`, say in its evidence what would settle it, and move on. Do not ask the person here, whatever `revui drive` prints about asking: that question belongs to a run they request, and the `revui-drive` skill asks it then.

A deployed environment only has merged code. It can show what happens today, before this change. What this change does has to come from its head: a local run, or a preview deploy of the branch.

**Diagrams**: only for the few items that are hard to follow in text (a flow across several components, an outcome that depends on ordering or state): `revui skill` prints how, under "Diagrams".

## 4. Finish

When the last part is reviewed:

1. Go back to every blocker that still has only `inferred` evidence and try to reproduce it, the way `revui skill` says under "Verify a finding". Set `working` to `Verifying <id>` while you do. A blocker ends up `confirmed` with what the run printed, `unreproduced` with what you ran, or untouched with one evidence line saying what would settle it. Leave one untouched only when the run needs a running product and `revui drive "$dir"` has no place recorded for this repo, or when it cannot be decided by running. The rule above still holds: do not ask the person where to run it.
2. Finish the summary, two short paragraphs at most: what the ticket asks and what the change does, with the placeholder line replaced by the findings that matter most, by id; then what you checked and what you did not, naming any blocker that is still unverified.
3. Remove `working`.
4. Run `revui check "$dir"` and fix what it reports until it prints `ok`.

Then say in a few lines what you found, by id. Do not paste the guide. Stay in the conversation: the person asks about findings, and has them verified, here.
