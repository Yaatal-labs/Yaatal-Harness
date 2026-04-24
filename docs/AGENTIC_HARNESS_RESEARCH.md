# Agentic Harness Research: Anthropic & MiniMax

## Executive Summary

Research into agentic harness patterns from Anthropic and MiniMax reveals convergent thinking on several key principles, despite different implementation approaches. Both companies independently arrived at similar architectural decisions: multi-agent patterns, memory/session management, tool abstraction, and the importance of harness engineering over model selection.

---

## My Understanding of Agentic Harnesses

An **agentic harness** is the execution and orchestration layer that enables an AI system to operate as an agent rather than a stateless responder. It is the **configurable runtime** that sits between the model and the world, managing:

- How tasks are received
- How context is assembled
- How models are selected
- How tools are invoked
- How state is maintained
- How permissions are enforced
- How failures are recovered from
- How outputs are returned

**Key Insight**: The harness is NOT the model, NOT the user interface, and NOT merely a tool registry—it is the operating substrate for agentic behavior.

---

## Anthropic's Key Learnings

### 1. **Generator-Evaluator Pattern (GAN-Style)**

Anthropic's research on long-running agents revealed that separating the agent doing the work from the agent judging it is a strong lever.

```
┌─────────────────────────────────────────────────────────────────┐
│                    THREE-AGENT ARCHITECTURE                      │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│    ┌──────────────┐     Sprint Contract     ┌──────────────┐  │
│    │   PLANNER    │ ──────────────────────▶ │  GENERATOR    │  │
│    │              │                         │              │  │
│    │ Creates spec │                         │ Builds one    │  │
│    │ from prompt  │ ◀────────────────────── │ feature at    │  │
│    │              │     Sprint Contract     │ a time        │  │
│    └──────────────┘                         └───────┬──────┘  │
│                                                       │        │
│                                                       ▼        │
│    ┌──────────────┐                         ┌──────────────┐  │
│    │  EVALUATOR    │ ◀──────────────────── │  HANDSHAKE   │  │
│    │              │     Test + Grade         │              │  │
│    │ Grades with  │ ──────────────────────▶ │ Generator    │  │
│    │ concrete     │     Pass/Fail           │ proposes +   │  │
│    │ criteria     │                         │ evaluator    │  │
│    └──────────────┘                         │ reviews      │  │
│                                              └──────────────┘  │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**Why this works**: Making a generator critical of its own work is hard; tuning a standalone evaluator to be skeptical is far more tractable.

### 2. **Context Window Management (Critical)**

| Problem | Solution |
|---------|----------|
| Context window fills up mid-task | Compaction (summarize early parts) |
| "Context anxiety" (premature wrapping up) | Context resets (clean slate with handoff artifact) |
| Context exhaustion | Context trimming (selective token removal) |

**Key Finding**: Claude Sonnet 4.5 exhibited "context anxiety" strongly—compaction alone wasn't sufficient. Context resets became essential. Opus 4.5 largely removed this behavior.

### 3. **Session State Management**

For long-running agents that work in discrete sessions:

```
┌─────────────────────────────────────────────────────────────────┐
│                    SESSION RECOVERY PROTOCOL                     │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Every coding agent runs these steps at session start:          │
│                                                                 │
│  1. [Tool Use] <bash - pwd>                                    │
│     → See working directory                                     │
│                                                                 │
│  2. [Tool Use] <read - claude-progress.txt>                    │
│     → Understand what was accomplished                          │
│                                                                 │
│  3. [Tool Use] <read - feature_list.json>                      │
│     → See what's left to do                                     │
│                                                                 │
│  4. [Tool Use] <bash - git log --oneline -20>                  │
│     → Review recent commits                                     │
│                                                                 │
│  5. Run init.sh                                                 │
│     → Start development server                                  │
│                                                                 │
│  6. Basic end-to-end test                                       │
│     → Verify app isn't in broken state                          │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 4. **Tool Design (ACI - Agent-Computer Interface)**

Anthropic spent **more time optimizing tools than prompts**. Key principles:

```rust
// BEFORE: Model struggled with relative paths
// Tool took relative paths, model made mistakes after directory changes

// AFTER: Always require absolute paths
// Result: Model used this flawlessly

// Key ACI Principles:
// 1. Put yourself in the model's shoes
// 2. Write excellent documentation (like a docstring for a junior dev)
// 3. Poka-yoke your tools (make mistakes harder to make)
// 4. Use clear parameter names and descriptions
// 5. Include examples, edge cases, input format requirements
// 6. Test extensively
```

### 5. **Decoupled Architecture ("Brain from Hands")**

```
┌─────────────────────────────────────────────────────────────────┐
│                    DECOUPLED ARCHITECTURE                        │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   ┌─────────────┐     execute(name, input)      ┌───────────┐ │
│   │    BRAIN    │ ─────────────────────────────▶│   HANDS   │ │
│   │  (Harness) │                                │ (Sandbox) │ │
│   │             │ ◀─────────────────────────────│           │ │
│   │ Claude +    │      string result            │ Container │ │
│   │ Orchestration                               │ or Tool   │ │
│   └──────┬──────┘                                └───────────┘ │
│          │                                                   │
│          │ getEvents(), emitEvent()                         │
│          ▼                                                   │
│   ┌─────────────┐                                           │
│   │   SESSION   │  (External Context Object)                │
│   │             │  - Append-only log                         │
│   │ Recoverable │  - Durable storage                        │
│   │ context     │  - Enables resume from crash              │
│   └─────────────┘                                           │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**Benefits**:
- Container dies → Harness catches failure as tool-call error → Claude can retry
- Nothing in harness needs to survive a crash
- Sandboxes become tools: `execute(name, input) → string`
- Hands can be passed between brains

---

## MiniMax's Key Learnings

### 1. **Self-Evolution Through Harness Modification**

MiniMax M2.7 treats the harness as mutable, not fixed:

```
┌─────────────────────────────────────────────────────────────────┐
│                  AUTONOMOUS OPTIMIZATION LOOP                    │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│    ┌─────────────┐                                             │
│    │   ANALYZE   │ ◀──── 100+ rounds executed                 │
│    │   failures  │                                             │
│    └──────┬──────┘                                             │
│           │                                                    │
│           ▼                                                    │
│    ┌─────────────┐                                             │
│    │    PLAN     │  • Sampling parameters (temp, freq penalty) │
│    │   changes   │  • Workflow guidelines                      │
│    └──────┬──────┘  • Loop detection optimizations             │
│           │          • Bug pattern searching                    │
│           ▼                                                    │
│    ┌─────────────┐                                             │
│    │   MODIFY    │                                             │
│    │   scaffold  │                                             │
│    └──────┬──────┘                                             │
│           │                                                    │
│           ▼                                                    │
│    ┌─────────────┐                                             │
│    │     RUN     │                                             │
│    │  evaluations│                                             │
│    └──────┬──────┘                                             │
│           │                                                    │
│           ▼                                                    │
│    ┌─────────────┐                                             │
│    │   COMPARE   │                                             │
│    │   results   │                                             │
│    └──────┬──────┘                                             │
│           │ keep or revert                                      │
│           └─────────────────────────────────────────────────────┘
│                                                                 │
│    Result: 30% performance improvement on internal eval sets   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### 2. **Agent Teams for Complex Tasks**

MiniMax implements multi-agent collaboration requiring:

| Requirement | Implementation |
|------------|---------------|
| Role boundaries | Separate agents with distinct system prompts |
| Adversarial reasoning | Agents challenge each other's logical blind spots |
| Protocol adherence | Structured communication via files |
| Behavioral differentiation | Each agent has stable role identity |
| State machine management | Agents make autonomous decisions within complex states |

### 3. **Skills System (2000+ token skills)**

- **40+ complex skills** implemented
- **97% skill adherence rate**
- Skills support file generation, multi-round editing on Word/Excel/PPT
- Skills/MCP implementation is part of self-evolving architecture

### 4. **Memory Mechanisms**

| Type | Implementation | Purpose |
|------|---------------|---------|
| Persistent memory | Long-term retention | Cross-session continuity |
| Short-term memory | Markdown files after each iteration | Round-by-round context |
| Self-feedback chain | All previous rounds inform next | Cumulative learning |

### 5. **ML Competition Harness (MLE Bench)**

```
┌─────────────────────────────────────────────────────────────────┐
│              ML COMPETITION HARNESS (Simple + Effective)        │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌─────────────────┐                                            │
│  │ SHORT-TERM     │  Agent generates markdown file after       │
│  │ MEMORY         │  each iteration round                      │
│  └─────────────────┘                                            │
│                                                                 │
│  ┌─────────────────┐                                            │
│  │ SELF-FEEDBACK  │  Performs self-criticism on current       │
│  │                 │  round's results                          │
│  └─────────────────┘                                            │
│                                                                 │
│  ┌─────────────────┐                                            │
│  │ SELF-          │  Next round conducts optimization based    │
│  │ OPTIMIZATION   │  on memory + feedback from all previous     │
│  │                 │  rounds                                    │
│  └─────────────────┘                                            │
│                                                                 │
│  Results: 66.6% medal rate (22 competitions)                   │
│  - 9 gold, 5 silver, 1 bronze                                 │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## Convergent Patterns (Both Anthropic + MiniMax)

Despite different implementations, both arrived at:

### 1. **Multi-Agent Decomposition**

| Anthropic Pattern | MiniMax Pattern |
|-------------------|-----------------|
| Generator + Evaluator | Planner + Generator + Evaluator |
| GAN-style adversarial | Agent Teams with role boundaries |
| Separate concerns | Separate concerns |

**Both agree**: Single agents try to do too much; decomposition improves outcomes.

### 2. **Session/Checkpoint Management**

| Anthropic | MiniMax |
|-----------|---------|
| claude-progress.txt | Short-term memory (markdown files) |
| feature_list.json | Self-feedback chain |
| git-based checkpointing | Persistent memory |
| init.sh | Environment initialization |

**Both agree**: Agents work in discrete sessions; must leave clear artifacts for recovery.

### 3. **Tool Abstraction**

| Anthropic | MiniMax |
|-----------|---------|
| `execute(name, input) → string` | MCP (Model Context Protocol) |
| Sandboxes as tools | Skills as tools (2000+ tokens) |
| Container provisioning via tool call | Dynamic tool search |

**Both agree**: Tool execution should be abstracted, controllable, and governed.

### 4. **Memory Hierarchy**

```
┌─────────────────────────────────────────────────────────────────┐
│                    MEMORY HIERARCHY                              │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  ┌───────────────────────────────────────────────────────────┐   │
│  │                    LONG-TERM MEMORY                      │   │
│  │   (Persistent across sessions, task-agnostic)          │   │
│  └───────────────────────────────────────────────────────────┘   │
│                            │                                    │
│                            ▼                                    │
│  ┌───────────────────────────────────────────────────────────┐   │
│  │                   SESSION MEMORY                         │   │
│  │   (Progress files, feature lists, git history)          │   │
│  └───────────────────────────────────────────────────────────┘   │
│                            │                                    │
│                            ▼                                    │
│  ┌───────────────────────────────────────────────────────────┐   │
│  │                  WORKING MEMORY                          │   │
│  │   (Context window, tool results, immediate feedback)    │   │
│  └───────────────────────────────────────────────────────────┘   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## Usable Patterns for Yaatal Harness

### 1. **Minimal Agent Loop**

```rust
// From MiniMax's mini-agent, simplified:
async fn run(&self) -> String {
    let mut step = 0;
    while step < self.max_steps {
        // 1. Check context length, summarize if needed
        await self.summarize_if_needed();

        // 2. Call LLM with tools
        let response = self.llm.generate(
            messages: self.messages,
            tools: self.tool_list,
        ).await?;

        // 3. If no tool calls, task complete
        if response.tool_calls.is_empty() {
            return response.content;
        }

        // 4. Execute tools, add results to history
        for tool_call in response.tool_calls {
            let result = self.execute_tool(tool_call).await?;
            self.messages.push(tool_message(tool_call, result));
        }

        step += 1;
    }
    "Max steps reached".to_string()
}
```

### 2. **Session Recovery Artifact**

```rust
// Every session starts by reading these files:
// 1. progress.txt - what was accomplished
// 2. feature_list.json - what's left to do
// 3. git log - recent commits
// 4. init.sh - how to start the environment

// Implement in yaatal-agent:
async fn resume_session(&self, session_id: &str) -> Result<SessionContext> {
    let events = self.session_store.get_events(session_id).await?;
    let progress = self.read_file("progress.txt").await?;
    let features = self.read_file("feature_list.json").await?;
    Ok(SessionContext { events, progress, features })
}
```

### 3. **Evaluator-Generator Loop**

```rust
// yaatal-agent pattern:
pub struct AgentLoop {
    generator: Arc<dyn LlmProvider>,
    evaluator: Arc<dyn LlmProvider>,
    tools: Arc<ToolExecutor>,
    session: Arc<dyn SessionStore>,
}

impl AgentLoop {
    pub async fn run_sprint(&self, spec: &TaskSpec) -> Result<SprintResult> {
        // Negotiate sprint contract
        let contract = self.negotiate_contract(spec).await?;

        // Generator builds
        let build = self.generator.build(&contract).await?;

        // Evaluator tests
        let evaluation = self.evaluator.evaluate(&build, &contract.criteria).await?;

        // Check thresholds
        if evaluation.passes_all() {
            Ok(SprintResult::Success(build))
        } else {
            // Generator refines based on feedback
            self.generator.refine(build, evaluation.feedback()).await
        }
    }
}
```

### 4. **Tool Abstraction with Sandboxing**

```rust
// yaatal-tools pattern:
#[async_trait]
pub trait Sandbox: Send + Sync {
    async fn execute(&self, cmd: &str, args: serde_json::Value) -> Result<String, ToolError>;
    async fn provision() -> Result<Self, ToolError>;
    fn id(&self) -> &str;
}

// Implementations:
// - LocalProcessSandbox (runs commands locally)
// - ContainerSandbox (runs in Docker/container)
// - RemoteSandbox (runs on remote machine)

pub struct ToolExecutor {
    sandboxes: RwLock<Vec<Box<dyn Sandbox>>>,
    // ... tool registry, permissions, etc.
}
```

### 5. **Memory Store with Tiering**

```rust
// yaatal-memory pattern:
#[async_trait]
pub trait MemoryStore: Send + Sync {
    // Working memory (within context window)
    async fn short_term(&self, key: &str) -> Result<Option<String>, MemoryError>;

    // Session memory (across sessions)
    async fn store(&self, entry: MemoryEntry) -> Result<String, MemoryError>;
    async fn recall(&self, id: &str) -> Result<Option<MemoryEntry>, MemoryError>;

    // Long-term memory (across projects)
    async fn persist(&self, entry: MemoryEntry) -> Result<(), MemoryError>;
    async fn search(&self, query: &str, limit: usize) -> Result<Vec<MemoryEntry>, MemoryError>;
}
```

---

## Edge Cases to Pay Attention To

### 1. **Context Anxiety** ⚠️

**Problem**: Models (especially Sonnet 4.5) exhibit premature wrapping up as context fills.

**Symptoms**:
- Declaring victory before work is complete
- Skipping thorough testing
- Not checking edge cases

**Solutions**:
- Use context resets (clean slate with handoff artifacts)
- Monitor token usage, trigger compaction before limits
- Explicit checkpoints with verification

### 2. **One-Shotting** ⚠️

**Problem**: Agents try to do too much at once.

**Symptoms**:
- Features half-implemented
- Context exhausted mid-implementation
- Next session must guess what happened

**Solutions**:
- Mandatory single-feature sprints
- Feature list with explicit completion criteria
- Progress file updated after each significant action

### 3. **Self-Evaluation Failure** ⚠️

**Problem**: Agents praise their own work even when quality is mediocre.

**Symptoms**:
- Marking features complete without proper testing
- Missing obvious bugs
- Confident but wrong

**Solutions**:
- Separate evaluator agent (Anthropic's key insight)
- Hard thresholds—if any criterion fails, sprint fails
- Never let the builder also be the judge

### 4. **Tool Path Issues** ⚠️

**Problem**: Relative paths break after directory changes.

**Symptoms**:
- File not found errors
- Incorrect file modifications
- Tool calls failing silently

**Solutions**:
- Always use absolute paths (Anthropic's ACI principle)
- Validate paths before operations
- Include working directory in tool context

### 5. **Session Loss** ⚠️

**Problem**: Container/session crashes lose all context.

**Symptoms**:
- Start from scratch each time
- Wasted work repetition
- Can't recover mid-task state

**Solutions**:
- External session store (append-only log)
- Git-based checkpointing
- Progress files + init scripts
- `wake(session_id)` → resume from last event

### 6. **Security Boundaries** ⚠️

**Problem**: Prompt injection can escape sandbox.

**Symptoms**:
- Agent reads credentials it shouldn't
- Tool calls modified by external input
- Token leakage

**Solutions**:
- Decouple brain from hands (Anthropic's architecture)
- Auth bundled with resources, not in sandbox
- Credential isolation via external vault
- MCP proxy for credential fetching

### 7. **Harness Staleness** ⚠️

**Problem**: Assumptions about what model can't do become outdated.

**Symptoms**:
- Scaffolding that's no longer needed
- Missing capabilities that model now supports
- Over-engineered solutions

**Solutions**:
- MiniMax's approach: Model rewrites harness
- Regular harness review on model upgrade
- Strip away load-bearing pieces that aren't

### 8. **Latency/Cost Explosion** ⚠️

**Problem**: Agent loops add latency and cost multiplicatively.

**Symptoms**:
- Simple tasks become expensive
- User waits for many round-trips
- Token usage grows exponentially

**Solutions**:
- Use workflows for predictable tasks (not agents)
- Max iteration limits
- Fallback to simpler patterns when possible
- Monitor cost per task

---

## Key Quotes to Remember

> "Harness engineering yields higher marginal returns than model selection."
> — MiniMax/Agentic Harness Community

> "The key insight is finding a way for agents to quickly understand the state of work when starting with a fresh context window."
> — Anthropic Engineering

> "Success in the LLM space isn't about building the most sophisticated system. It's about building the right system for your needs."
> — Anthropic Engineering

> "The space of interesting harness combinations doesn't shrink as models improve—it moves."
> — Anthropic Engineering

---

## Implementation Recommendations for Yaatal

### Immediate (MVP)

1. **Implement session recovery** with progress files and feature lists
2. **Add Generator-Evaluator pattern** for yaatal-agent
3. **Tool path handling**: Always use absolute paths
4. **Context monitoring**: Track token usage, trigger compaction/resets
5. **Max step limits**: Prevent infinite loops

### Short-term

1. **Memory tiering**: Short-term + long-term memory stores
2. **Sandbox abstraction**: LocalProcessSandbox, ContainerSandbox
3. **MCP support**: Native tool protocol integration
4. **Git checkpointing**: Commit after each session

### Medium-term

1. **Multi-agent patterns**: Planner + Generator + Evaluator
2. **Self-evolution**: Let model modify harness parameters
3. **Skills system**: 2000+ token skill definitions
4. **Agent Teams**: Role-bound, adversarial reasoning

### Monitoring

1. **Cost tracking**: Per-task, per-sprint, per-session
2. **Quality metrics**: Pass/fail rates, iteration counts
3. **Token usage**: Context window utilization
4. **Recovery rates**: How often sessions resume successfully

---

## References

- [Anthropic: Harness Design for Long-Running Apps](https://www.anthropic.com/engineering/harness-design-long-running-apps)
- [Anthropic: Effective Harnesses for Long-Running Agents](https://www.anthropic.com/engineering/effective-harnesses-for-long-running-agents)
- [Anthropic: Scaling Managed Agents](https://www.anthropic.com/engineering/managed-agents)
- [Anthropic: Building Effective Agents](https://www.anthropic.com/research/building-effective-agents)
- [MiniMax: M2.7 Self-Evolution](https://www.minimax.io/news/minimax-m27-en)
- [MiniMax: Mini-Agent Framework](https://platform.minimax.io/docs/solutions/mini-agent)
- [Agentic Harnesses as Infrastructure](https://medium.com/@balajibal/agentic-harnesses-the-new-infrastructure-layer-for-ai-systems-3939c6fac1a6)
