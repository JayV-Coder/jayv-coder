# Jev AI Orchestrator CLI

Act as a **Senior Software Engineer, Software Architect, and AI Infrastructure Engineer**.

Design and implement a terminal-based AI development environment similar in experience to OpenCode, but with a fundamentally different architecture.

The main orchestration engine must be **Jev AI + RAG**.

Jev must sit between the user and the available LLMs and decide how each user request should be processed before any expensive model is invoked.

The primary objective is to reduce:

* token consumption;
* unnecessary context transmission;
* unnecessary LLM calls;
* expensive model usage;
* duplicated reasoning;
* long-running provider sessions;
* repeated repository analysis.

At the same time, routing optimization must not significantly reduce answer quality.

The architecture should support both **API-based providers** and **locally installed AI CLIs**, such as Claude Code when available on the machine.

---

# 1. Core Architecture

The fundamental architecture should be:

```text
                         ┌──────────────────────────┐
                         │          USER            │
                         │        Terminal          │
                         └────────────┬─────────────┘
                                      │
                                      ▼
                         ┌──────────────────────────┐
                         │       Jev AI CLI         │
                         │   TUI / Chat / Commands  │
                         └────────────┬─────────────┘
                                      │
                                      ▼
                    ┌─────────────────────────────────┐
                    │       JEV ORCHESTRATOR          │
                    │                                 │
                    │  Intent Classification          │
                    │  Complexity Analysis            │
                    │  Task Planning                  │
                    │  Context Selection              │
                    │  Provider Selection             │
                    │  Model Selection                │
                    │  Token Budgeting                │
                    │  Tool Selection                 │
                    └───────┬──────────┬──────────────┘
                            │          │
                    ┌───────┘          └────────┐
                    ▼                           ▼
             ┌─────────────┐             ┌─────────────┐
             │     RAG     │             │    Tools    │
             │             │             │             │
             │ Repository  │             │ Filesystem  │
             │ Memory      │             │ Git         │
             │ Docs        │             │ Shell       │
             │ Symbols     │             │ Browser     │
             └──────┬──────┘             └──────┬──────┘
                    │                           │
                    └─────────────┬─────────────┘
                                  │
                                  ▼
                    ┌───────────────────────────┐
                    │      Model Router         │
                    └─────────────┬─────────────┘
                                  │
              ┌───────────────────┼────────────────────┐
              │                   │                    │
              ▼                   ▼                    ▼
        ┌───────────┐       ┌───────────┐       ┌───────────┐
        │ Providers │       │ Local LLM │       │ CLI Agent │
        │           │       │           │       │           │
        │ OpenAI    │       │ LM Studio │       │ Claude    │
        │ Anthropic │       │ Ollama    │       │ Codex     │
        │ OpenRouter│       │ vLLM      │       │ Others    │
        └───────────┘       └───────────┘       └───────────┘
```

Jev must be the **control plane**.

External LLMs are workers.

The user should interact primarily with Jev, not manually decide which model should receive every request.

---

# 2. Fundamental Principle

The system must not behave like:

```text
User
 ↓
Send entire conversation + repository
 ↓
Expensive LLM
 ↓
Response
```

Instead:

```text
User Prompt
 ↓
Jev
 ↓
Understand request
 ↓
Retrieve relevant context
 ↓
Estimate complexity
 ↓
Determine required capability
 ↓
Select provider/model
 ↓
Construct minimal context
 ↓
Execute
 ↓
Validate
 ↓
Return result
```

Jev must answer one fundamental question before delegating:

> **What is the cheapest and smallest execution strategy capable of completing this task with sufficient quality?**

Cost includes more than API price.

Consider:

* input tokens;
* output tokens;
* latency;
* local compute;
* provider quota;
* context-window usage;
* session limits;
* task complexity;
* tool capabilities.

---

# 3. Jev Responsibilities

Jev should not merely classify prompts.

It should act as an **AI orchestration engine**.

For every user request, determine:

```text
intent
complexity
domain
required_capabilities
required_context
tools
model_class
provider
token_budget
execution_strategy
```

Example internal decision:

```json
{
  "intent": "code_fix",
  "domain": "backend",
  "complexity": "medium",
  "context_required": [
    "src/auth",
    "package.json",
    "AGENTS.md"
  ],
  "capabilities": [
    "code",
    "reasoning",
    "tool_use"
  ],
  "strategy": "single_agent",
  "preferred_model_class": "coding-medium",
  "max_context_tokens": 12000
}
```

This structure is conceptual. Use the format that best fits Jev's actual capabilities.

---

# 4. Provider Abstraction

Create a provider abstraction so the orchestrator does not depend directly on a specific vendor.

Conceptually:

```text
Provider
 ├── APIProvider
 ├── OpenAIProvider
 ├── AnthropicProvider
 ├── OpenRouterProvider
 ├── OpenAICompatibleProvider
 ├── LocalProvider
 └── CLIProvider
```

A provider should expose capabilities such as:

```text
chat()
stream()
supportsTools()
supportsVision()
supportsReasoning()
contextWindow()
estimatedCost()
health()
```

Do not scatter provider-specific logic throughout the application.

---

# 5. OpenAI-Compatible Providers

Support generic OpenAI-compatible endpoints.

This enables integration with:

* LM Studio;
* vLLM;
* RunPod;
* custom inference servers;
* compatible proxies.

Example configuration:

```yaml
providers:

  lmstudio:
    type: openai-compatible
    base_url: http://127.0.0.1:1234/v1
    api_key: optional

  runpod:
    type: openai-compatible
    base_url: ${RUNPOD_URL}
    api_key: ${RUNPOD_API_KEY}
```

This should make local and remote inference interchangeable from Jev's perspective.

---

# 6. CLI Providers

One of the important differentiators should be support for locally installed AI CLIs.

Examples:

```text
Claude Code
Codex CLI
Gemini CLI
other compatible agents
```

Implement a CLI provider abstraction.

Conceptually:

```text
CLIProvider
 ├── detect()
 ├── version()
 ├── capabilities()
 ├── execute()
 ├── stream()
 └── cancel()
```

At startup, Jev can detect available integrations:

```text
Checking AI providers...

✓ Claude CLI
✓ LM Studio
✓ OpenAI
✗ Gemini CLI
✓ RunPod
```

If Claude Code is installed and authenticated, Jev may route an appropriate task to it instead of consuming an Anthropic API key directly.

Do not assume that every CLI supports identical input/output, session, tool, or streaming semantics.

Create adapters.

---

# 7. Model Registry

Maintain a model registry independent from providers.

Example:

```yaml
models:

  local-fast:
    provider: lmstudio
    model: qwen
    capabilities:
      - chat
      - code
    cost_class: free
    speed: fast

  coding-premium:
    provider: anthropic
    model: configured-model
    capabilities:
      - code
      - reasoning
      - tools
    cost_class: high

  reasoning-premium:
    provider: openai
    model: configured-model
    capabilities:
      - reasoning
      - tools
    cost_class: high
```

Avoid hardcoding specific current model names throughout the routing engine.

Models change frequently.

Capabilities should drive routing.

---

# 8. Model Capability Profiles

Each configured model should expose metadata such as:

```text
coding
reasoning
vision
tool_use
long_context
speed
cost
reliability
local
privacy
```

Example:

```yaml
models:

  qwen-local:
    capabilities:
      coding: 0.75
      reasoning: 0.60
      tools: true

    cost:
      input: 0
      output: 0

    context_window: 32768
```

Scores may be manually configured initially.

Later they can be learned from execution telemetry.

---

# 9. Routing Engine

The Model Router should combine:

```text
Task requirements
+
Model capabilities
+
Provider availability
+
Token budget
+
Cost
+
Latency
+
User preferences
+
Historical performance
```

Conceptually:

```text
score(model, task) =
    capability_match
  + historical_success
  + availability
  - estimated_cost
  - expected_latency
  - context_penalty
```

Do not make routing depend solely on prompt length.

A short request can require deep reasoning.

A long request can be trivial summarization.

---

# 10. Routing Strategies

Support different execution strategies.

### DIRECT

Jev can answer without another LLM.

```text
User
 ↓
Jev
 ↓
Answer
```

Use for deterministic operations or tasks Jev can reliably solve.

### SINGLE_MODEL

```text
User
 ↓
Jev
 ↓
Model
 ↓
Response
```

### TOOL_FIRST

```text
User
 ↓
Jev
 ↓
Tool
 ↓
Relevant data
 ↓
Model
```

### RAG_FIRST

```text
User
 ↓
Jev
 ↓
RAG
 ↓
Relevant context
 ↓
Model
```

### MULTI_STAGE

```text
User
 ↓
Jev
 ↓
Cheap Model
 ↓
Analysis
 ↓
Premium Model
 ↓
Final Result
```

### SPECIALIST

```text
User
 ↓
Jev
 ↓
Task classification
 ↓
Coding Agent / Security Agent / UX Agent / etc.
```

---

# 11. Escalation

Implement model escalation.

Do not immediately use the strongest model.

Example:

```text
Jev
 ↓
Local model
 ↓
Confidence sufficient?
 ├── YES → return
 └── NO
      ↓
   Medium model
      ↓
   sufficient?
      ├── YES
      └── NO → Premium model
```

However, do not force cheap-model execution when Jev already determines that the task clearly requires a stronger model.

Otherwise the system wastes tokens instead of saving them.

---

# 12. RAG as the Context Layer

RAG is central to the architecture.

The primary purpose is to prevent sending the entire repository to every model.

Build a repository knowledge layer.

Index:

```text
source files
documentation
AGENTS.md
README
configuration
API definitions
database schemas
symbols
classes
functions
dependencies
Git metadata
```

The RAG pipeline should conceptually be:

```text
Repository
 ↓
Parser
 ↓
Semantic Chunking
 ↓
Embeddings
 ↓
Vector Store
 ↓
Metadata Index
```

---

# 13. Code-Aware RAG

Do not rely exclusively on arbitrary fixed-size text chunks.

For source code, prefer semantic units:

```text
File
Class
Function
Method
Interface
Type
Module
Route
Component
SQL object
```

Maintain metadata:

```text
repository
path
language
symbol
symbol_type
dependencies
imports
exports
git_hash
last_modified
```

This allows Jev to retrieve much more precise context.

---

# 14. Hybrid Retrieval

Use more than vector similarity.

Combine:

```text
Semantic Search
+
Lexical Search
+
Symbol Search
+
Dependency Graph
+
Recent Files
+
Git Changes
```

Example:

User:

```text
Fix authentication token refresh.
```

Instead of sending 400 files:

```text
Jev
 ↓
search("token refresh")
 ↓
auth-service.ts
token-manager.ts
session.ts
auth.test.ts
AGENTS.md
```

Only those files or relevant fragments should reach the worker model.

---

# 15. Incremental Indexing

Do not re-index the entire repository after every change.

Use:

```text
File Hash
Git status
mtime
```

to identify changed files.

Pipeline:

```text
File changed
 ↓
Parse changed file
 ↓
Replace affected chunks
 ↓
Update embeddings
 ↓
Update symbol/dependency metadata
```

This keeps RAG inexpensive.

---

# 16. Context Builder

Implement a dedicated Context Builder.

Its responsibility is to build the smallest useful context for a model.

Example:

```text
System Instructions
+
AGENTS.md relevant rules
+
Task
+
Retrieved Code
+
Relevant Conversation Summary
+
Tool Results
```

Do not automatically send complete conversation history.

---

# 17. Hierarchical Memory

Use multiple memory layers.

```text
Working Memory
Session Memory
Project Memory
Semantic Memory
```

### Working Memory

Current execution only.

### Session Memory

Current chat decisions.

### Project Memory

Stable information such as:

```text
architecture
framework
coding conventions
important modules
project rules
```

### Semantic Memory

RAG-indexed knowledge.

This prevents repeatedly paying tokens to rediscover the same project architecture.

---

# 18. Conversation Compression

Long chats must not continuously increase provider context.

Periodically transform:

```text
100 previous messages
```

into:

```text
Session Summary
+
Active Decisions
+
Open Tasks
+
Relevant References
```

Keep raw history locally when required, but do not automatically send it all to worker models.

---

# 19. Prompt Compiler

Create a Prompt Compiler between orchestration and providers.

```text
Jev Decision
+
Context Builder
+
Provider Requirements
 ↓
Prompt Compiler
 ↓
Optimized Provider Prompt
```

It should:

* remove irrelevant context;
* remove duplication;
* apply provider-specific formatting;
* inject relevant project rules;
* respect token budgets;
* preserve essential constraints.

---

# 20. Token Budget Manager

Every request should have a budget.

Example:

```yaml
budgets:
  trivial: 2000
  simple: 5000
  medium: 12000
  complex: 30000
```

Before execution:

```text
estimate_input_tokens()
estimate_output_tokens()
estimate_cost()
```

Jev can then decide whether to:

```text
compress context
retrieve fewer chunks
use another model
split task
escalate
```

---

# 21. Session Budget

Also track consumption per session.

Example TUI:

```text
Session

Requests: 27
Input:    84K tokens
Output:   12K tokens
Estimated cost: $0.42
Saved by RAG: ~310K tokens
```

The savings number must be based on measurable methodology rather than invented estimates.

---

# 22. Tools

The orchestrator should support tools independently from the worker LLM.

Initial tools:

```text
filesystem
read
write
edit
search
grep
git
shell
tests
browser
```

Permissions should be configurable.

Example:

```yaml
permissions:

  read: allow
  search: allow

  write: ask
  shell: ask

  git:
    status: allow
    diff: allow
    commit: ask
    push: deny
```

---

# 23. Tool Execution

Prefer:

```text
LLM requests tool
 ↓
Jev validates permission
 ↓
Jev executes tool
 ↓
Result returned to model
```

instead of giving models unrestricted system access.

Jev remains the security boundary.

---

# 24. Agents

Support specialized agents.

Example:

```yaml
agents:

  developer:
    capabilities:
      - coding
      - reasoning
      - tools

  frontend:
    capabilities:
      - frontend
      - ux
      - browser

  security:
    capabilities:
      - security
      - reasoning

  reviewer:
    capabilities:
      - code-review
```

Jev decides which agent is appropriate.

Agents should define behavior and capabilities, not necessarily one fixed model.

---

# 25. Skills

Support skills similar to:

```text
senior-frontend
senior-backend
senior-fullstack
senior-architect
senior-security
database-designer
playwright-pro
code-review
```

Load skills **on demand**.

Do not inject every skill into every prompt.

Flow:

```text
Task
 ↓
Jev classification
 ↓
Relevant skills
 ↓
Load skill instructions
 ↓
Prompt Compiler
```

This itself reduces context consumption.

---

# 26. Provider Configuration

Example project configuration:

```yaml
jev:

  default_strategy: auto

  optimization:
    minimize_tokens: true
    prefer_local: true
    allow_escalation: true

providers:

  lmstudio:
    type: openai-compatible
    base_url: http://127.0.0.1:1234/v1

  openai:
    type: openai
    api_key: ${OPENAI_API_KEY}

  anthropic:
    type: anthropic
    api_key: ${ANTHROPIC_API_KEY}

  claude-cli:
    type: cli
    command: claude
    auto_detect: true
```

Credentials must use secure OS credential storage or environment-variable mechanisms appropriate to the platform.

Never persist secrets in plaintext project configuration.

---

# 27. TUI

The terminal interface should be comparable in usability to modern coding CLIs.

Conceptually:

```text
┌───────────────────────────────────────────────────────┐
│ Jev                                                   │
│ project: pixchaos                         main        │
├───────────────────────────────────────────────────────┤
│                                                       │
│ > Fix authentication persistence                     │
│                                                       │
│ Jev                                                   │
│ Searching project context...                         │
│                                                       │
│ Found 4 relevant files                               │
│ Strategy: coding-agent                               │
│ Provider: local                                      │
│                                                       │
│ Analyzing authentication flow...                     │
│                                                       │
├───────────────────────────────────────────────────────┤
│ Tokens 4.2K │ Context 12% │ Cost $0.00 │ agent:auto │
└───────────────────────────────────────────────────────┘
```

Do not overwhelm normal users with orchestration internals.

Provide an optional verbose/debug mode.

---

# 28. Explainable Routing

Provide:

```text
/why
```

to explain the latest routing decision.

Example:

```text
Task: backend code modification
Complexity: medium

Selected:
  Agent: developer
  Provider: LM Studio
  Model: local-coder

Reasons:
  ✓ coding capability
  ✓ sufficient context window
  ✓ local provider available
  ✓ estimated task complexity within threshold

Estimated:
  Input: 6.4K
  Output: 2K
```

This will be extremely valuable while tuning Jev.

---

# 29. Manual Overrides

Automatic routing must not remove user control.

Support commands such as:

```text
/model auto

/model local-coder

/provider anthropic

/agent developer

/budget 15000
```

Default:

```text
/model auto
```

Jev controls routing unless explicitly overridden.

---

# 30. Observability

Every execution should generate structured telemetry locally.

Track:

```text
task_type
complexity
selected_agent
selected_model
selected_provider
retrieved_chunks
input_tokens
output_tokens
latency
tool_calls
success
failure
escalation
estimated_cost
```

Never log secrets.

This data will later allow the router to improve based on real usage.

---

# 31. Routing Evaluation

Create a benchmark suite for Jev.

Example tasks:

```text
Explain function
Fix TypeScript error
Create React component
Refactor service
Analyze architecture
Security review
Write unit tests
Debug runtime failure
```

Compare:

```text
Auto Routing
vs.
Always Premium Model
```

Measure:

```text
quality
tokens
cost
latency
success rate
```

Otherwise there is no objective evidence that Jev actually improves efficiency.

---

# 32. Critical Optimization Principle

Do not optimize exclusively for the lowest number of tokens.

The actual objective should be:

```text
minimize total execution cost
subject to
acceptable task quality
```

For example, this is bad:

```text
cheap model
→ fails
→ retry
→ fails
→ premium model
```

if Jev could have identified from the beginning that the task required the premium model.

Routing quality is therefore as important as context compression.

---

# 33. Recommended Internal Architecture

Use clear boundaries.

```text
apps/
  cli/

core/
  orchestrator/
  router/
  context/
  memory/
  rag/
  agents/
  skills/
  tools/
  sessions/

providers/
  openai/
  anthropic/
  openai-compatible/
  cli/
  lmstudio/

infrastructure/
  database/
  vector-store/
  filesystem/
  git/
  telemetry/

ui/
  tui/
```

Exact directories must follow the selected language/framework conventions.

The important requirement is dependency separation.

---

# 34. Recommended Execution Pipeline

The central pipeline should become:

```text
USER INPUT
    │
    ▼
┌─────────────────────┐
│ Input Normalization │
└──────────┬──────────┘
           ▼
┌─────────────────────┐
│ Jev Intent Analyzer │
└──────────┬──────────┘
           ▼
┌─────────────────────┐
│ Complexity Analyzer │
└──────────┬──────────┘
           ▼
┌─────────────────────┐
│ Context Planner     │
└──────────┬──────────┘
           │
      ┌────┴────┐
      ▼         ▼
     RAG      Tools
      │         │
      └────┬────┘
           ▼
┌─────────────────────┐
│ Context Builder     │
└──────────┬──────────┘
           ▼
┌─────────────────────┐
│ Strategy Selector   │
└──────────┬──────────┘
           ▼
┌─────────────────────┐
│ Model Router        │
└──────────┬──────────┘
           ▼
┌─────────────────────┐
│ Prompt Compiler     │
└──────────┬──────────┘
           ▼
┌─────────────────────┐
│ Provider Adapter    │
└──────────┬──────────┘
           ▼
        LLM / CLI
           │
           ▼
┌─────────────────────┐
│ Result Validator    │
└──────────┬──────────┘
           │
      ┌────┴──────┐
      │ sufficient│
      └────┬──────┘
       YES │ NO
           │  └────→ Escalation
           ▼
        RESPONSE
```

This pipeline should be the architectural heart of the project.

---

# 35. MVP

Do not implement the entire architecture simultaneously.

Build the MVP in phases.

## Phase 1 — CLI Core

Implement:

```text
TUI
Sessions
Configuration
Provider abstraction
OpenAI-compatible provider
One API provider
Claude CLI adapter
Basic Jev router
```

Goal:

```text
User → Jev → Correct Provider → Response
```

## Phase 2 — Repository Intelligence

Implement:

```text
Repository scanner
AGENTS.md discovery
Code parser
Embeddings
Vector store
Hybrid retrieval
Context builder
Incremental indexing
```

Goal:

```text
User → Jev → Relevant Repository Context → LLM
```

## Phase 3 — Optimization

Implement:

```text
Token budgets
Conversation compression
Context deduplication
Model escalation
Cost tracking
/why
```

## Phase 4 — Agentic Development

Implement:

```text
Tools
Permissions
Agents
Skills
Git
Shell
Tests
Browser
```

## Phase 5 — Adaptive Routing

Use telemetry to improve routing.

Conceptually:

```text
Task
→ Selected Model
→ Result
→ Success/Failure
→ Cost
→ Latency
→ User Feedback
→ Routing History
```

Jev can gradually learn which models perform best for which classes of tasks.

---

# 36. Definition of Success

The project succeeds when a user can enter a repository and execute:

```text
jev
```

then ask:

```text
> Analyze this project and fix the authentication issue.
```

without manually deciding:

```text
which files to send
which model to use
which provider to use
how much context to send
which skill to load
whether RAG is necessary
whether another model should be consulted
```

Jev should determine those decisions.

The resulting system should behave conceptually as:

```text
                 USER
                   │
                   ▼
              ┌─────────┐
              │   JEV   │
              └────┬────┘
                   │
        ┌──────────┼──────────┐
        │          │          │
        ▼          ▼          ▼
       RAG       MEMORY      TOOLS
        │          │          │
        └──────────┼──────────┘
                   ▼
               ROUTER
                   │
       ┌───────────┼───────────┐
       ▼           ▼           ▼
     LOCAL       API         CLI
      LLM        LLM        AGENT
       │           │           │
       └───────────┼───────────┘
                   ▼
              VALIDATION
                   │
             ┌─────┴─────┐
             │           │
           ACCEPT      ESCALATE
             │           │
             ▼           └──→ stronger model
           USER
```

Jev is therefore not merely another chatbot.

It is the **orchestration, context, routing, memory, security, and optimization layer between the developer and an ecosystem of LLMs**.

The architectural priority must remain:

> **Send the minimum necessary context to the least expensive capable model, while maintaining sufficient quality to complete the task correctly.**

Do not begin by building dozens of agents or provider integrations. Build the **Orchestrator + Provider Abstraction + RAG + Context Builder + measurable routing telemetry** correctly first. Those components determine whether the project actually reduces token and session consumption or merely adds another abstraction layer on top of existing LLMs.
