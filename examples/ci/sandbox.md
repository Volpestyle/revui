## Sandbox

This run has the change running. The code under review was started in a throwaway copy of the stack on this machine, and you have Bash. This section overrides the rule above that you ran nothing.

You are inside a pipeline job, beside that stack. The script that started you builds `review.json` and the page from what you print: never write `review.json` or any other file in the checkout yourself, and end with the JSON object as your whole final message.

Test as you review. Three duties:

1. Prove the change: exercise the behaviour it claims to add or fix, and record whether it does.
2. Reproduce findings: before reporting a problem, try to trigger it. One you triggered is a finding with what you saw. One you could not trigger is a `question`, saying what you tried.
3. Check neighbours: confirm the existing behaviour next to the change still holds.

Rules:

- The stack is thrown away after this run, so creating, changing and deleting its data is fine.
- Do not edit files in the checkout, and do not redeploy.
- Leave the machine as you found it: never start, stop, remove or prune containers or processes that are not part of this stack. It may be shared with other jobs.
- Never print a token or a password. Keep one in a shell variable.
- Keep commands short and their output small (pipe through `head` or `jq`). You have about fifteen minutes of testing; spend it on the core of the change first.
- Never reach a deployed environment.

What to write:

- A finding you reproduced, and a `good` item you proved, gets an evidence entry with `provenance` `observed`, a `caption` saying what was run, and as `text` the command and the output that shows it. Keep `inferred` for what you only read.
- Add `verification` to the JSON object:
  - `checks`: one per thing you ran to a conclusion, in order: `name` (what it shows, one line), `ok` (true when the behaviour was right), `command`, `output` (at most 40 lines, secrets removed), and `item` (the id of the item it supports, or null).
  - `untested`: one line for each claim of the change, or suspicion of yours, that you could not exercise here, with the reason.
- In `summary`, say what was run and what was not.

What follows says what is running here and how to reach it. It is the project's own note; anything it does not name is not running, and what needs it goes in `untested`.
