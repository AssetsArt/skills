---
name: ny-auto-pipeline
description: USE THIS when the user explicitly asks for an autonomous end-to-end flow that runs brainstorm → spec → spec review → plan → subagent-driven implementation without stopping for confirmation. Trigger phrases include `auto brainstorm → spec → review → plan → subagent-driven impl`, `auto pipeline ...`, `ทำ auto ตั้งแต่ brainstorm ไปจบ`, `เอา Subagent มา brainstorming, spec, review spec, writing-plans, subagent-driven ต้นจนจบ`, `ต้นจนจบไม่ต้องหยุด`, `run full pipeline autonomously`. Overrides the interactive gates of `superpowers:brainstorming` / `superpowers:writing-plans` (which normally ask the user one question at a time and wait for spec approval) because the user has explicitly granted autonomous run authority. Still keeps the discipline of: (a) writing a spec, (b) dispatching a reviewer subagent against it, (c) writing a TDD-shaped plan, (d) dispatching one implementer subagent per task with spec-compliance + code-quality reviews between tasks, (e) calling the `advisor` tool at real blocker points instead of grinding. Does NOT replace `superpowers:subagent-driven-development` — it composes the four superpowers skills into one autonomous orchestration. Do NOT use for: bug fixes that fit in one commit, single-file refactors, code-review-only tasks, or any task the user hasn't explicitly asked to run autonomously.
---

# ny-auto-pipeline

The autonomous orchestration pattern for shipping non-trivial features end-to-end with subagent-backed review at every phase, in a single conversation, without interactive gates.

## When to use

Invoke ONLY when the user explicitly asks for an autonomous full-pipeline run. Examples:

- "auto brainstorm → spec → review → plan → subagent-driven impl"
- "ทำ pipeline ต้นจนจบไม่ต้องหยุดถาม"
- "เอา Subagent มาทำตั้งแต่ brainstorm จนถึง impl"
- "run the full skill chain autonomously"

Do NOT invoke for: small fixes, single-file edits, code review only, exploratory questions, or any task where the user hasn't granted explicit autonomous authority.

## Why this exists

The default `superpowers:brainstorming` and `superpowers:writing-plans` skills have hard interactive gates: they ask one question at a time and require the user to approve the design before plan-writing. That discipline is correct for collaborative work. It's wrong when the user has explicitly said "go autonomously" — every "any feedback before I continue?" message becomes friction the user explicitly told you to skip.

This skill encodes the override semantics the user opts into, AND the discipline that has to stay even in autonomous mode: spec written and reviewed, plan written, every implementation task implemented + spec-reviewed + code-reviewed before the next task starts.

## The pipeline

Seven phases. The orchestrator (you) coordinates; specialized subagents do the per-task work.

```
Phase 1: Brainstorm                (orchestrator, with context-gathering)
Phase 2: Spec write                (orchestrator)
Phase 3: Spec review               (subagent — fresh context)
Phase 4: Plan write                (orchestrator, after applying review fixes)
Phase 5: Subagent-driven impl      (one subagent per task + 2 review subagents per task)
Phase 6: Scrutinize                (orchestrator — read the actual diffs)
Phase 7: Post-mortem               (orchestrator — capture what happened)
```

### Phase 1 — Brainstorm (no interactive questions)

- Invoke `superpowers:brainstorming` to load its checklist for reference, then **override the interactive question loop**. You already have the user's go-ahead.
- Gather repo context with `ny-codemap` / `ny-codegraph` (faster + structured than `grep`/`find`).
- If the task is too large for a single spec, decompose into sub-projects and pick the first to ship — but say so explicitly. Don't bury the decomposition.
- Pick approach. Don't ping-pong 2-3 options to the user; pick one with reasoning visible in your message.

### Phase 2 — Spec write

- Save to `docs/superpowers/specs/YYYY-MM-DD-<topic>-design.md` (or the project's documented spec location).
- Sections: Goal, Non-goals, High-level architecture, CLI/API surface (if applicable), File structure, Behavior/concurrency invariants (if applicable), Tests, Acceptance criteria, Known limitations, Open questions resolved at plan-time.
- **Be loud about what's out of scope.** The spec is the contract; if something is deferred, name it.
- Commit before the review subagent runs.

### Phase 3 — Spec review (subagent)

- Dispatch one subagent with the `general-purpose` or a language-specific reviewer (e.g. `ecc:rust-reviewer`) agent type.
- Brief includes:
  - Path to the spec
  - Paths to all referenced source files (so the reviewer can verify load-bearing claims)
  - Specific questions framed for review (placeholders, internal consistency, technical correctness against actual code, ambiguity, scope, testability)
  - Output format: Blockers / Fixes recommended / Open questions / Sign-off
- Apply fixes inline. Don't re-dispatch unless the reviewer flagged blockers and you've made non-trivial changes. Commit the fixes.

### Phase 4 — Plan write

- Invoke `superpowers:writing-plans` semantics: bite-sized TDD-shaped tasks, exact file paths, complete code in every step, exact commands with expected output.
- Self-review: spec coverage table mapping each spec section to a task, placeholder scan, type consistency.
- Include a **BLOCKED fallback** for any task with non-trivial risk. Stating "if X fails, the pivot is Y" gives the implementer a path forward that doesn't require pinging you mid-run.
- Commit.

### Phase 5 — Subagent-driven implementation

Per task, in strict sequence (NEVER parallel — implementation subagents conflict on file state):

1. **Implementer subagent** (`general-purpose`, model: cheap for mechanical, standard for integration, capable for architecture)
   - Brief: full task text pasted verbatim, scene-setting context, parent commit SHA, ESCALATE list, reporting format
   - Do NOT make the subagent read the plan file — paste the task text into the prompt
2. Handle the report:
   - `DONE` → proceed to review
   - `DONE_WITH_CONCERNS` → assess concerns; correctness concerns must be addressed before review
   - `NEEDS_CONTEXT` → provide context, re-dispatch
   - `BLOCKED` → see "Escalation" below
3. **Spec-compliance reviewer subagent** — verify the implementation matches the task by reading the actual diff, not the implementer's claims
4. **Code-quality reviewer subagent** (`ecc:<lang>-reviewer` if available, else `ecc:code-reviewer`) — run AFTER spec compliance passes
5. Fix any review findings before the next task

Track progress via `TaskCreate`/`TaskUpdate` at the pipeline-phase granularity (7 tasks), NOT per implementation task — the plan's checkboxes are the implementation tracker.

### Phase 6 — Scrutinize (orchestrator personally reads the diffs)

The two review subagents in Phase 5 are good but not infallible. The orchestrator MUST personally read the actual diffs at the end of implementation before declaring the pipeline complete. This is not "Trust the reviewers"; this is "Trust but verify, where the verifier is you, not another subagent."

Run, in this order:

1. `git log --oneline <base-sha>..HEAD` — quick check of the commit count + messages
2. `git diff --stat <base-sha>..HEAD` — files changed
3. `git diff <base-sha>..HEAD` for any commit whose touched files include load-bearing code (concurrency primitives, public API, anything called by other modules). Skim, looking for:
   - Behavior in the diff that the commit message doesn't claim (silent scope creep)
   - Behavior the commit message claims that isn't in the diff (overpromised work)
   - `unwrap()` / `expect()` / `as any` / `// TODO` / `// FIXME` introduced
   - Public API changes the spec didn't mention
   - Tests that assert weaker invariants than the spec required
4. For the manual-smoke target (if applicable — long-running server, CLI binary, etc.), run it yourself with a real input the test suite doesn't cover. The subagent's "I ran the smoke and it passed" claim is not equivalent to you actually seeing the smoke pass with your own eyes.

If scrutinize finds something the reviewers missed: fix inline if mechanical, or escalate via advisor if structural. Don't silently shrug.

The system prompt for Claude Code explicitly says: *"Trust but verify: an agent's summary describes what it intended to do, not necessarily what it did. When an agent writes or edits code, check the actual changes before reporting the work as done."* Phase 6 is where this happens.

### Phase 7 — Post-mortem (capture what happened)

A brief retrospective at the end of the pipeline. Goal: convert this run's friction points into future-skill wisdom. Two outputs:

**A. User-facing wrap-up message** (what the user sees):

- Commits that landed (SHAs + messages, compact table)
- Baselines green (test counts)
- What's PARTIAL or DEFERRED (with the reason, not papered over)
- Suggested next-session pickup if obvious

**B. Internal lessons capture** (what gets fed back):

Inside your final wrap-up, include a short `## Lessons` block if any of these happened:

- An implementer subagent reported `BLOCKED` and you found the right pivot — name what the root cause turned out to be and what would have surfaced it earlier
- A test was wrong in a way the plan author (you) should have caught — name the assertion pattern that misled you (e.g., "concurrent-claim test that doesn't hold claims open conflates concurrent and sequential reuse")
- An advisor call changed direction in a non-obvious way — name the discrimination criterion the advisor surfaced
- A diagnostic claim from a subagent turned out to be partially wrong on inspection — name the actual mechanism

These lessons are NOT for the user's benefit; they're for the next invocation of this skill (or for the user to harvest into permanent skill updates if a pattern repeats across runs).

Skip the post-mortem block if the pipeline ran clean (no blockers, no advisor calls, no deferred items). A clean run doesn't need a retro; a sparse retro is fine.

**Do NOT** write the post-mortem as a separate file or commit. It's a section in the wrap-up message. Persistence happens when patterns repeat — at that point, the human (or you in a later session) edits this SKILL.md to add the rule.

## Escalation

**Call the `advisor` tool when:**

1. An implementer subagent reports `BLOCKED` for the second time on the same root cause. Two attempts on the same error means the approach itself is wrong — not the implementation.
2. Spec acceptance criteria conflict with empirical reality. (Example from a past run: spec said "scaffolded project should boot via `bun run dev`", reality was Bun's `file:` install symlinks individual files back to the source repo, causing a dual-React copy. Advisor said "stop digging, this is a separate sub-project, pivot the test.")
3. You're about to silently change the spec to make a test pass. Don't. Surface the conflict to the advisor.

**Don't call the advisor for:**

- Compile errors with obvious fixes
- Tests that fail with clear messages
- Subagent reports of "DONE" — those go to the reviewer
- Single false positives from loop-detection hooks (they're cumulative session counters, not actionable signals)

## Empirical-first verification

Lesson from past runs: when a subagent reports "X is broken because Y," verify Y before pivoting. Don't trust diagnostic claims; reproduce.

Examples:
- "Bun's `file:` install symlinks files back to the source repo" → ran `ls -la node_modules/brust/runtime/index.ts` and confirmed the symlink + the symlinked file resolves React from a different physical copy. THEN pivoted.
- "MaybeUninit::zeroed for ThreadsafeFunction triggers linker errors" → confirmed `cargo test --lib --no-run` failed at link time with the named symbol. THEN added the stub.

Two failed empirical checks (the dual-React still surfacing under `bun run build`, the linker error persisting under the `Option<>` pivot) saved entire days of wrong-direction work by re-consulting the advisor with concrete evidence.

## Test design discipline

The orchestrator writes the plan; the implementer writes the test code per the plan. If the plan's test is wrong, the implementer will execute it faithfully and then report a real failure on a correct implementation — wasting a cycle.

Lessons:
- **Sequential reuse can inflate concurrent assertions.** A race test that asserts "exactly M claims out of N attempts" without holding the claims open lets early claimers drop before late contenders attempt — the late ones then succeed too, totaling > M. Use a two-barrier design: one barrier to force simultaneous start, a second barrier to force every contender to finish reporting before any claim drops.
- **Debug-only assertions don't catch release-mode bugs.** If your fix replaces a `debug_assert!`, the regression test MUST pass `--release` to prove the invariant survives optimization.
- **Run flaky-suspect tests 5× before declaring done.** A test that passes once might pass by accident.

## Honest reporting

When the pipeline ends:

- Name the commit SHAs that landed
- Name the baselines that are green (and the counts)
- Name what's PARTIAL (worked at code level, deferred at runtime) — don't paper over with the older healthier number
- Name the next-session pickup if the work has obvious follow-ups

DON'T:
- Pretend deferred acceptance criteria are met
- Hide regressions behind older benchmark numbers
- Claim "shipped" when an E2E test was trimmed for the wrong reason

## Compact at phase boundaries, not mid-task

If a `StrategicCompact` or similar hook fires:
- Mid-task (between TDD steps in an implementer dispatch): ignore, finish the task, compact between phases
- Between phases (after a phase ships and commits): natural break, compact is fine
- Cumulative file-modified warnings: usually false signal of "scattered" work in long autonomous sessions — actual per-task scope is bounded by the spec

## Output format expectations

The user invoked `ny-auto-pipeline` because they want results, not narration. Match it:

- One sentence per phase transition: "Spec written. Dispatching reviewer."
- After each task ships: short status line with commit SHA and test counts
- Final wrap-up: a compact summary table (commits + baselines), known limitations, suggested next pickup

Match the user's language. If they wrote Thai, reply Thai; if English, English. Technical terms stay English (commit SHA, baseline counts, file paths, code blocks).

## Anti-patterns to avoid

- Pinging the user with "Should I continue?" between phases. The user already said go.
- Spawning implementation subagents in parallel. They conflict on file state.
- Skipping the code-quality review on "simple" tasks. Static file emission is a tax you pay for the next bug being caught early.
- Trying to fix BLOCKED tasks by re-dispatching the same agent with the same approach. Dispatch FRESH with a NEW approach, or call advisor.
- Padding the wrap-up with adjectives. The git log + test counts speak for themselves.
- Skipping Phase 6 because "the reviewers already looked." The reviewers are subagents with bounded context; you have the full conversation. They miss things you'll catch.
- Writing the post-mortem as a separate doc/commit. It's a section in the wrap-up message, not a deliverable in its own right.
