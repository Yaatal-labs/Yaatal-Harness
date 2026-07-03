# CLI-First Tools for Yaatal Agents

Status: design proposal. No binary named below exists yet. The pattern is
adapted from ["Why CLIs Beat MCP for AI Agents — And How to Build Your Own CLI
Army"](https://medium.com/@rentierdigital/why-clis-beat-mcp-for-ai-agents-and-how-to-build-your-own-cli-army-6c27b0aec969)
(Phil, Rentier Digital, Medium, Feb 2026), which cites Peter Steinberger
(OpenClaw) — "mcp were a mistake. bash is better" — and his own CLI-army
practice (`goplaces`, `imsg`, `bird`, `wacli`, and others, each doing one thing,
each scriptable, each documented in a `SKILL.md`). This document takes that
pattern and states what it means for a workspace that already has an MCP
server (`Yaatal-SDK`) and a runtime that must never hand an agent bare shell
access (this repo's whole reason to exist).

## Stance

**CLI-first for agent-facing tools executed by local/self-hosted runtimes.**
Headless cron agents (Engine), the livestream agent loop (Studio), and any
future harness-driven runtime should reach for a small, sharp CLI over a
running service whenever the target is something Yaatal itself operates.

**MCP stays the right shape for hosted-connector contexts** — claude.ai
connectors, remote integrations, OAuth-fronted third parties, anything where
the caller is a hosted client that can't exec a local binary and needs a
protocol handshake instead. `Yaatal-SDK` already ships a minimal MCP server and
a tool-manifest generator for exactly that lane; nothing here proposes removing
it. The two lanes serve different callers, not competing philosophies.

A caveat on the article's numbers: the 30–40%-of-context-window cost it
attributes to MCP servers is from a specific measurement, at a specific point
in tool-loading maturity. Harnesses that support deferred or lazy tool
loading — including the very harness rendering this document — load a tool's
full schema only when it's invoked, not up front, which changes that math
considerably. Treat the article's cost figures as directional evidence that
schema-dumping is expensive, not as a number to cite verbatim in a design
review. The rest of the case for CLIs — one exec call, testable in seconds,
composable via pipes, no server process to keep alive — holds regardless of
how any particular harness loads schemas.

## The Yaatal CLI army (all proposed)

Four CLIs, each scoped to one subsystem, each a thin front end over an
existing or planned business layer — not a new business layer of its own.

| CLI | Wraps | Example commands |
|---|---|---|
| `yaatal` | The Engine kernel — products, orders, deliveries, payment status. Grows from the `yaatal_api-cli` binary Engine already has. | `yaatal orders list --status pending --json`, `yaatal payments status <order_id> --json`, `yaatal deliveries assign <order_id> <courier_id>` |
| `yaatal-studio` | Livestream ops in Yaatal-Studio — the OBS-driving agent loop's own actions, exposed as commands instead of only being reachable from inside that loop. | `yaatal-studio session start --json`, `yaatal-studio product switch <sku> --json`, `yaatal-studio mark-sold-out <sku>`, `yaatal-studio clip --last 30s --json` |
| `yaatal-evals` | Runs a suite from `yaatal-evals` (starting with the one `RunEval` described in `docs/CONTROL-LOOP.md`) and scores it. | `yaatal-evals run merchant-metrics-daily --json`, `yaatal-evals list --json` |
| `yaatal-audit` | Queries the audit store described in `docs/CONTROL-LOOP.md` — read-only. | `yaatal-audit runs --since 24h --json`, `yaatal-audit show <run_id> --json`, `yaatal-audit proposals --pending --json` |

Each one:

- does one thing well (no `yaatal-swiss-army-knife`),
- takes a `--json` flag for structured output,
- has a `--help` that fully teaches the tool in one page,
- returns exit codes that mean something (0 success, non-zero failure, and the
  failure modes distinguished where it matters — e.g. "denied by policy" vs.
  "tool crashed" should not share an exit code),
- takes no interactive prompts — every input is a flag or a file, because an
  agent cannot answer a `(y/n)` on stdin.

`yaatal` is the one with an existing seed: Engine's `yaatal_api-cli` binary.
Growing the kernel CLI out of that binary — rather than starting a fifth
process that talks to the same database — is the concrete first step, not a
parallel build.

## Harness custody makes CLI-first safe

The article's blind spot, by its own admission, is that `execSync(commands[name])`
with no gate in between is fine for a solo builder's own machine and not fine
for a multi-tenant commerce platform moving money. Bare shell access for an
agent is a different risk profile than a human running commands in their own
terminal. Yaatal does not give agents bare bash. The CLI army above is only
as safe as three things this repo already has the shape of:

1. **The tool contract.** `yaatal-tools` already defines the shape a tool must
   have: `Tool::metadata()` (a `ToolMetadata` with typed `ToolParameter`s) and
   `Tool::execute()`. Today's built-in tools include a raw `ShellTool` — useful
   as a scaffold, not something an agent should be pointed at in production
   (the workspace README already flags "dangerous local tools" as something
   near-term cleanup should move behind examples/feature flags). The CLI army
   is the replacement: a **`CliTool`** wrapper (proposed, `yaatal-tools`) that
   declares one specific binary path, an argument schema (not a free-form
   command string), a timeout, and a cost class — the same `Tool` trait, a
   narrower implementation. An agent calling `yaatal-audit runs --since 24h`
   through a `CliTool` cannot pivot to running arbitrary shell the way it could
   through `ShellTool`.
2. **The policy layer.** `yaatal-policy`'s proposed `ToolPolicy` trait (see
   `docs/CONTROL-LOOP.md`) is what turns "a CLI exists" into "an agent may call
   this CLI, with these arguments, up to this spend." The allowlist and
   per-run spend cap apply to CLI invocations exactly as they apply to any
   other tool call — a `CliTool` is a `Tool`, so it goes through the same gate.
3. **The audit wrapper.** Every `CliTool` invocation goes through
   `ToolExecutor::execute`, which — once the audit spine from
   `docs/CONTROL-LOOP.md` lands — emits an `AuditEvent` per call: which binary,
   which arguments (or their digest), exit code, duration, cost, and the
   policy verdict that allowed it to run.

CLIs alone give ergonomics: fast to write, fast to test, cheap on context, no
protocol overhead. The Harness custody layer — contract, policy, audit — is
what makes handing that ergonomic surface to an autonomous agent a reasonable
thing to do instead of a liability. Neither half is the Yaatal position by
itself; the combination is.

## Agent-friendly CLI rules

Adapted from the article's three rules, plus two Yaatal-specific additions for
tools that can change state:

1. **`--json` for structured output.** No box-drawing tables as the only
   output format. An agent parses JSON; it should not be asked to regex a
   pretty-printed table.
2. **`--help` that fully teaches the tool in one page.** Every flag, every
   default, every example, the shape of `--json` output. An agent reads
   `--help` the way a human skims a README — if it's incomplete, the agent
   guesses, and a guessed flag on a mutating command is the failure mode this
   whole design exists to prevent.
3. **Exit codes that mean something.** 0 is success. Non-zero is not success —
   never exit 0 while swallowing an error. Where the failure has a distinct
   cause an agent should react to differently (denied by policy vs. execution
   failure vs. timeout), use distinct codes rather than collapsing everything
   into 1.
4. **Idempotent by default, or explicitly flagged as destructive.** A command
   an agent might retry after an ambiguous result (timeout, dropped
   connection) should be safe to run twice — or its `--help` and its name
   should make the destructive, non-retriable nature obvious (`--force`, a verb
   like `cancel` or `delete` rather than something that reads as safe).
5. **`--dry-run` for anything that mutates.** `yaatal orders cancel <id>
   --dry-run` should report what would happen without doing it. This is the
   one rule with no equivalent in the source article, because the article's
   CLIs (metrics checks, deploys, Slack posts) either don't mutate durable
   state or are cheap to reverse. Yaatal's do mutate durable state — orders,
   deliveries, payments — so a mutating command without a `--dry-run` path is
   not agent-ready yet, independent of how good its `--help` is.

## Documentation pattern

The article's core move — document CLIs in `CLAUDE.md` so an agent knows what
exists without discovering it by trial and error — applies unchanged. As each
CLI above ships, its repo's `CLAUDE.md` (or `AGENTS.md`, same pattern, either
name) grows an **"Available CLIs"** section:

```
## Available CLIs

### yaatal (Engine kernel CLI)
- `yaatal orders list --status pending --json` - list orders needing action
- `yaatal payments status <order_id> --json` - check a payment's rail/state
- `yaatal deliveries assign <order_id> <courier_id>` - assign a courier

### yaatal-audit
- `yaatal-audit runs --since 24h --json` - recent runs across all runtimes
- `yaatal-audit show <run_id> --json` - one run's full event trail
- `yaatal-audit proposals --pending --json` - proposals awaiting human review
```

Each entry is a command plus a one-line description of what it's for — not a
copy of `--help`'s full output. The section grows only as a CLI actually ships;
an entry for a CLI that doesn't exist yet is exactly the kind of "described as
existing" this design explicitly avoids elsewhere in this document, and should
avoid in practice too.
