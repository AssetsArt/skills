---
name: ny-principal-advisor
description: "Consult a Principal Software Engineer / Architect — a fresh-context advisor subagent dispatched on **Fable** (fallback **Opus**) — at real decision points, whatever model runs the current session. Use when: architecture/design tradeoffs aren't resolvable from the code; BLOCKED twice on the same root cause; spec conflicts with empirical reality; about to make a hard-to-reverse structural change (public API, data model, concurrency design, dependency boundary). Trigger phrases: `ask the advisor`, `consult the architect`, `second opinion on this design`, `ปรึกษา advisor`, `ขอความเห็น architect`, `ถาม principal ก่อน`, `ขอ second opinion`, and any time `ny-auto-pipeline` says to call the advisor. The advisor ADVISES; the session model (and ultimately the user) DECIDES. Do NOT use for routine implementation questions, obvious errors, decisions the user already made, or as a substitute for reading the code yourself."
---

# ny-principal-advisor

## Overview

A consultation pattern, not a delegation pattern. When the session hits a real
decision point — architecture, tradeoff, repeated blocker, spec-vs-reality
conflict — dispatch **one advisor subagent** that plays a Principal Software
Engineer / Architect with fresh context, and have it return a structured
recommendation. The advisor always runs on the top capability tier (**Fable**,
fallback **Opus**) even if the session model is Sonnet or Haiku: judgment is
exactly the work you never down-tier (see the capability ladder in
`ny-orchestrate-subagents`).

**Core principle:** the advisor sees the problem with no sunk cost and no
attachment to the current approach. You bring the evidence; it brings the
judgment; you keep the decision.

## When to consult

- Two or more credible designs and the tradeoff isn't resolvable from the code alone.
- A second `BLOCKED` on the same root cause — two failed attempts mean the
  *approach* is suspect, not the implementation.
- Spec or acceptance criteria conflict with what the code/tools actually do.
- You're about to make a hard-to-reverse move: public API surface, persisted
  data shape, concurrency model, workspace/dependency boundary, spec rewrite.
- You notice you're rationalizing ("just this once", "probably fine") on a
  structural choice.

**When NOT to consult:**
- The user already decided — follow the user, don't shop for a counter-opinion.
- The answer is in the code — read it (use `ny-codemap`/`ny-codegraph`) first.
- Routine fixes, obvious errors, style questions, anything a reviewer subagent
  already covers.
- More than ~2 consultations per task: if you keep needing the advisor, the
  task is under-specified — surface that to the user instead.

## Dispatch mechanics

Use the **Agent tool**, foreground (you cannot proceed without the answer, so
blocking is correct here — this is the exception to `ny-orchestrate-subagents`'
background-by-default rule):

- `subagent_type: "general-purpose"` (or `Plan` for pure design questions —
  it's read-only and architecture-oriented).
- `model: "fable"`. If the dispatch errors because Fable is unavailable,
  retry once with `model: "opus"`. Never lower.
- One advisor per decision. Don't panel-poll three advisors and average —
  if you need adversarial pressure, ask the one advisor for the strongest
  case *against* its own recommendation (the `DISSENT` field below).

## The brief (self-contained — the advisor can't see this chat)

```
You are a Principal Software Engineer / Architect acting as an advisor.
You advise; you do not implement. Recommend, with explicit tradeoffs.

DECISION QUESTION: <one sentence — the actual choice to make>

CONTEXT: <project shape, constraints, relevant paths the advisor may read>

OPTIONS CONSIDERED:
  A) <option> — evidence for/against, what was already tried, exact errors
  B) <option> — ...

CONSTRAINTS: <hard requirements, spec quotes, things the user already decided>

MY CURRENT LEAN: <your tentative pick and why — be honest, it sharpens the reply>

Reply in exactly this shape:
RECOMMENDATION: <pick one option, or name a better option C>
RATIONALE: <the load-bearing reasons, tied to the evidence above>
RISKS: <what breaks or hurts later if we follow this>
DISSENT: <the strongest argument against this recommendation, and what
         evidence would flip the call>
```

Paste real evidence — exact error output, spec lines, file paths — not your
summary of it. A vague brief returns a vague oracle. Include your current lean:
hiding it doesn't make the advisor more objective, it just makes the answer
less targeted.

## Handling the answer

1. **You decide; the advisor advises.** Weigh the `RECOMMENDATION` against the
   conversation context the advisor doesn't have. Adopting it is your call and
   stays your responsibility — never report "the advisor made me do it".
2. **`DISSENT` is the test.** Before acting, check whether the flip-evidence
   the advisor named actually exists in your context. If it does, the
   recommendation just falsified itself — surface that.
3. **Advisor contradicts an explicit user decision → stop and ask the user.**
   Don't silently follow either side.
4. **Advisor says "stop digging, pivot"** (the classic second-BLOCKED outcome)
   → treat the current approach as dead. Re-dispatch fresh with the new
   approach; don't blend the corpse of the old one into it.
5. Record one line in your running notes/plan: decision, advisor's
   recommendation, what you chose, why — so the post-mortem (Phase 7 of
   `ny-auto-pipeline`) has the trail.

## Relationship to sibling skills

- **`ny-auto-pipeline`** names advisor call-points (second BLOCKED, spec vs
  reality, about-to-bend-the-spec). This skill is the concrete mechanism for
  those calls.
- **`ny-orchestrate-subagents`** owns tiering and background dispatch for
  *work*. The advisor is the deliberate exception: always top tier, usually
  foreground, never doing the work itself.

## Anti-patterns

- **Decision laundering** — consulting so the choice isn't "yours". It still is.
- **Oracle-shopping** — re-asking with a friendlier brief until you get the
  answer you wanted.
- **Skipping the evidence** — sending "X doesn't work, what should we do?"
  with no error output. The advisor will guess, confidently.
- **Down-tiering** — a Sonnet "advisor" is just a second implementer opinion.
  Fable/Opus or don't bother.
- **Consulting instead of reading** — if `codegraph impact` answers the
  question, the advisor is overhead.
- **Implementing through the advisor** — the moment the brief says "and write
  the code", it's not an advisor dispatch anymore; use a normal implementer
  subagent per `ny-orchestrate-subagents`.
