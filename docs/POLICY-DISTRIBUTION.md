# Policy Distribution — Portable Artifacts + In-Path Enforcement

> Status: design. Nothing here is implemented; the pattern is adopted, the machinery is not.

## The observation

[Ponytail](https://github.com/DietrichGebert/ponytail) (MIT) is a working specimen of the
Harness thesis, seen from the outside: a **behavioral policy pack** (its efficiency ladder —
rules about what an agent may generate), **evals** (a benchmark suite proving the policy doesn't
break correctness), and an **audit artifact** (`/ponytail-debt`, a ledger of deferred decisions).

What makes it interesting for us is the *distribution model*. It ships no server and no framework
to link against. One canonical policy, rendered into portable instruction files per runtime —
`.claude-plugin/`, `AGENTS.md`, `.cursor/rules/`, `.windsurf/rules/`, `.clinerules/`,
`.kiro/steering/`, `.openclaw/skills/` — reaching 16+ agent platforms it does not control.

## The pattern for Yaatal

Yaatal's behavioral policies (tool allowlists, spend caps, sovereignty constraints,
brand/language rules) should ship the same two-tier way:

1. **Portable tier — policy as instruction artifacts.** One canonical policy source in this repo,
   rendered into per-runtime rules files (CLAUDE.md sections, AGENTS.md, OpenClaw skills, …).
   This buys *reach*: any runtime that reads instructions gets the policy, including runtimes the
   Harness does not custody. It is advisory — an agent can ignore it.

2. **Enforcement tier — policy in the execution path.** For runtimes the Harness custodies,
   `yaatal-policy` checks the same rules in-path (see `CONTROL-LOOP.md` § policy gate): tool
   allowlist, argument constraints, spend caps. This buys *guarantees* — non-optional, audited,
   and it backstops exactly the gap the portable tier leaves open.

Same policy, two renderings. The canonical source must be one artifact so the tiers cannot
drift: the portable files and the `yaatal-policy` rules should both be generated from (or
verified against) it.

## First concrete instance

The Ponytail development policy itself, installed 2026-07 as a "Development policy" section in
each code repo's CLAUDE.md (Engine, Harness, SDK, Studio). That is the portable tier working for
the *code-generation* lane today. The enforcement tier for the *operational* lane (agents running
Yaatal, not building it) arrives with the CONTROL-LOOP milestones.

## Non-goals

- No policy DSL until at least two real policies exist in both tiers (ladder rung 1).
- The portable tier never carries secrets or tenant-specific limits — those are enforcement-tier
  configuration only.
