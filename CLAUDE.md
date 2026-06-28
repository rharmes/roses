- **Credentials never live in the repo.** Keep a defensive `.gitignore` entry for the config filename 
  in case a copy strays in.
- **Very low tolerance for flaky tests.** A test that passes only *sometimes* is a defect — in the
  test or the app — not noise to shrug off. Fix the root cause: wait for the real condition instead
  of a fixed timeout, click-and-verify-with-retry on a flaky control, freeze animations, or surface
  a genuine app race. CI `retries` are a backstop for truly unavoidable timing — **never the fix.**
- **Keep the always-loaded instructions lean.** `CLAUDE.md` (and any auto-loaded docs) is read every
  session, so it should hold the summary, the non-obvious constraints, and the workflow — push deep
  detail into reference docs and link to them rather than inlining it.
- **Keep docs current in the same commit.** When architecture or tooling changes significantly,
  update `CLAUDE.md` and the relevant doc in the *same* commit as the change.
- **Edit the checkout in place** for background sessions — don't spin up git worktrees unless requested.
- **Explain the tooling, not just the result.** Provide detailed walkthroughs of dev/ops tooling
  and conventions (CI, package manager, shell, deploy) — the reasoning, not only working code.
- **Suggest guardrails when a pattern emerges.** If the same kind of command keeps coming up,
  suggest a permission allow-rule or a small wrapper script to remove the repeated prompt.
