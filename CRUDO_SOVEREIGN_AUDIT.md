# Executive Summary

Crudo is a Rust CLI agent harness with a real streaming conversation loop, model-provider abstraction, permission-gated tools, sessions, hooks, plugins, subagents, MCP lifecycle code, and a separate RAG HTTP service.

It is not currently a sovereign multimodal industrial workbench. The architecture is extensible enough to reach that target, but several requirements require core changes:

- Native multimodal message support
- Integrated document ingestion and OCR
- Integrated RAG retrieval
- Office artifact generation and verification
- Strong host-level sandboxing
- Model routing by task/modality
- Network-deny-by-default enforcement
- Complete audit observability

Current readiness: **5/10**  
After minimal extensions: **7/10**  
After full implementation: **9/10**

Classification:

| Category | Assessment |
|---|---|
| Already implemented | Agent loop, streaming, sessions, tools, permissions, local OpenAI-compatible models, stdio MCP, plugins, hooks, basic sandboxing |
| Partially implemented | Subagents, plan mode, provider fallback, RAG, PDF text extraction, sandbox isolation, telemetry |
| Existing extension mechanism | Tools, plugins, MCP, skills, hooks, project configuration |
| Easy to add | Local MCP document/RAG/artifact tools, organization skills, rule-based model routing outside core, audit dashboard over existing events |
| Substantial development | Native vision, robust sandbox, integrated RAG, office generation, security enforcement |
| Not supported by current architecture alone | Native vision/PDF image understanding without changing API types; guaranteed air-gap security without host/container policy enforcement |

The repository was audited read-only. No files were modified, no packages were installed, and no network requests were made during the audit. The worktree already contains extensive user changes; those were preserved.

# Crudo Architecture

## Entry Point

| Component | Location | Details |
|---|---|---|
| Binary entry point | `rust/crates/crudo-cli/src/main.rs:330` | `main()` provides the top-level error envelope |
| CLI dispatch | `rust/crates/crudo-cli/src/main.rs:995` | `run()` parses and dispatches `CliAction` |
| CLI parser | `rust/crates/crudo-cli/src/main.rs:1478` | Hand-written parser, not Clap or another parser framework |
| Interactive REPL | `rust/crates/crudo-cli/src/main.rs:7048` | `LiveCli::run_repl` and turn handling |
| Alternate harness | `rust/crates/crudo-analog/src/lib.rs` | Separate lightweight API/runtime harness |

The main binary is named `crudo`; the package is `crudo-cli`.

## Main Execution Flow

```text
crudo prompt / REPL
    -> parse_args
    -> resolve cwd, model, permissions, configuration
    -> construct ProviderClient
    -> construct ConversationRuntime
    -> construct tool registry/executor
    -> load system prompt and context files
    -> send streaming request
    -> assemble assistant text/tool calls
    -> permission and hook checks
    -> execute tools
    -> append tool results to session
    -> repeat until no tool call
    -> persist session and render final output
```

Relevant code:

- `rust/crates/crudo-cli/src/main.rs:995`
- `rust/crates/crudo-cli/src/main.rs:7048`
- `rust/crates/runtime/src/conversation.rs:130`

## Agent Loop

`ConversationRuntime<C, T>` is the core loop:

`rust/crates/runtime/src/conversation.rs:130`

It:

1. Adds the user message to the session.
2. Sends the entire conversation to the API client.
3. Reconstructs streamed assistant text, thinking blocks, and tool calls.
4. Persists the assistant message.
5. Runs `PreToolUse` hooks.
6. Applies permission policy.
7. Executes each tool.
8. Runs post-tool hooks.
9. Adds tool results to the conversation.
10. Repeats until the model emits no tool calls.
11. Performs automatic compaction when configured thresholds are reached.

This is a genuine multi-step agent loop.

## Planning

There are two different planning mechanisms:

- `EnterPlanMode` / `ExitPlanMode` tools modify a worktree-local permission/configuration override.
- `TodoWrite`, task packets, workers, and subagents provide structured task state.

Relevant locations:

- `rust/crates/tools/src/lib.rs:769`
- `rust/crates/tools/src/lib.rs:629`
- `rust/crates/runtime/src/task_packet.rs`
- `rust/crates/runtime/src/task_registry.rs`

There is no dedicated deterministic planner/DAG engine. Planning is primarily model-directed.

## Reasoning

The API types support Anthropic thinking blocks, OpenAI-compatible reasoning content, `reasoning_effort`, and preservation of reasoning content in history where supported.

Locations:

- `rust/crates/api/src/types.rs:90`
- `rust/crates/api/src/types.rs:160`
- `rust/crates/api/src/types.rs:255`
- `rust/crates/crudo-cli/src/main.rs:1084`

Reasoning is delegated to the selected model. Crudo does not implement symbolic reasoning or a separate reasoning engine.

## Tool Calling

Tool definitions are converted to provider-compatible JSON schemas and sent in `MessageRequest.tools`.

Locations:

- `rust/crates/api/src/types.rs:8`
- `rust/crates/api/src/types.rs:120`
- `rust/crates/tools/src/lib.rs:484`
- `rust/crates/tools/src/lib.rs:1363`

The tool boundary is:

```rust
Result<String, String>
```

Tools are dispatched by name through `execute_tool`.

## Model Interface

The model interface is represented by:

- `Provider` trait: `rust/crates/api/src/providers/mod.rs:18`
- `ProviderClient`: `rust/crates/api/src/client.rs`
- `MessageRequest` / `MessageResponse`: `rust/crates/api/src/types.rs:8`

Supported wire protocols:

- Anthropic Messages
- OpenAI Chat Completions

## Provider Abstraction

Current provider modules:

- `rust/crates/api/src/providers/anthropic.rs`
- `rust/crates/api/src/providers/openai_compat.rs`
- `rust/crates/api/src/providers/mod.rs`

The provider enum is currently:

```rust
Anthropic
Xai
OpenAi
```

OpenAI-compatible configuration is used for OpenAI, xAI, DashScope/Qwen, Ollama, and arbitrary compatible gateways.

## Sessions and Context

Session state is implemented by:

- `Session`: `rust/crates/runtime/src/session.rs:117`
- `SessionStore`: `rust/crates/runtime/src/session_control.rs`
- `ConversationRuntime`: `rust/crates/runtime/src/conversation.rs:130`

Sessions persist JSONL conversations under `.crudo/sessions`.

Context includes conversation messages, system prompt, project instruction files, tool results, usage, compaction metadata, and prompt-cache information.

Compaction is implemented in `rust/crates/runtime/src/compact.rs`.

## Memory

Implemented:

- Session transcript memory
- Prompt history
- Session persistence
- Compaction
- Agent manifests/output files
- Task/worker state files

Not found:

- Long-term semantic memory integrated into the main conversation runtime
- Automatic RAG-backed memory
- Knowledge graph memory
- User/profile memory abstraction

The standalone RAG service is not automatically queried by `ConversationRuntime`.

## Configuration

Configuration is loaded by `ConfigLoader`:

`rust/crates/runtime/src/config.rs:409`

Precedence includes:

- User: `~/.crudo/settings.json`
- Legacy/user compatibility: `.crudo.json`
- Project: `.crudo/settings.json`
- Local: `.crudo/settings.local.json`

Supported configuration areas include `model`, `provider`, `providerFallbacks`, `permissions`, `sandbox`, `hooks`, `plugins`, `mcpServers`, `aliases`, `rulesImport`, and API timeouts.

## Permissions and Security

Primary components:

- `rust/crates/runtime/src/permissions.rs`
- `rust/crates/runtime/src/permission_enforcer.rs:27`
- `rust/crates/runtime/src/policy_engine.rs`
- `rust/crates/runtime/src/bash_validation.rs`
- `rust/crates/runtime/src/sandbox.rs`

Permission modes include ReadOnly, WorkspaceWrite, DangerFullAccess, Prompt, and compatibility aliases such as `plan`, `acceptEdits`, and `dontAsk`.

Permissions are tool-level and command-heuristic based. They are not a complete operating-system security boundary.

## File Handling

File operations are implemented in `rust/crates/runtime/src/file_ops.rs`.

Implemented:

- Text reads
- Text writes
- String replacement edits
- Glob search
- Regex/grep search
- Binary-file rejection for built-in text reads
- File-size limits
- Workspace containment variants
- Symlink escape checks

Not implemented as native file tools:

- Move
- Copy
- Delete
- Directory watch
- General binary reading
- Structured office file handling

## Command Execution

Shell execution is implemented in `rust/crates/runtime/src/bash.rs:72` and exposed by `rust/crates/tools/src/lib.rs:2296`.

Additional execution includes the REPL, PowerShell, background tasks, workers, and Git command wrappers.

By default commands execute as host subprocesses. The sandbox may wrap them in Linux `unshare`, but fallback execution is host `sh -lc`.

## Plugin/Extension System

Plugins are implemented in:

- `rust/crates/plugins/src/lib.rs`
- `rust/crates/plugins/src/hooks.rs`

Plugins can provide manifest metadata, tools, commands, hooks, lifecycle initialization/shutdown, and permission declarations.

The manifest convention is:

```text
plugin-root/.claude-plugin/plugin.json
```

The plugin system can load external and bundled plugins. It does not provide a dynamic model-provider ABI.

## Skill System

Skill resolution is implemented primarily in:

- `rust/crates/tools/src/lib.rs:4000`
- `rust/crates/tools/src/lib.rs:5480`
- `rust/crates/commands/src/lib.rs:3135`

Recognized roots include `.crudo/skills`, `.omc/skills`, `.agents/skills`, `~/.omc/skills`, `~/.claude/skills/omc-learned`, and legacy command roots.

Skill format is Markdown, normally:

```markdown
---
name: document-analysis
description: Analyze industrial documents
---

Instructions and workflow guidance.
```

Skills provide instructions and prompts. They do not define native Rust tools. A skill can direct the model to use MCP tools or built-in tools, but this is prompt-level composition rather than a formal skill dependency API.

Project-local skills are supported. Organization-wide skills can be distributed through a shared root or plugin/package deployment.

Existing repository production skills: **NOT FOUND IN REPOSITORY.**

# Existing Capabilities

Ranked strengths:

1. Real iterative tool-calling loop in `ConversationRuntime::run_turn`.
2. Provider abstraction with OpenAI-compatible support.
3. Workspace-aware file operations.
4. Permission policy and pre/post hooks.
5. Streaming responses.
6. Session persistence and compaction.
7. Plugin tools and lifecycle.
8. Subagents and worker/task infrastructure.
9. MCP stdio tool/resource integration.
10. Standalone RAG service.

# Existing Tools

All built-in tools are declared in `rust/crates/tools/src/lib.rs:484`; implementations dispatch through `rust/crates/tools/src/lib.rs:1363`. Exact inline JSON schemas are in the registry and dedicated Rust input structs begin around line 2765.

## A. Filesystem

| Tool | Function | Input/output | Permission | Air-gap |
|---|---|---|---|---|
| `read_file` | `run_read_file` | `path`, optional `offset`, `limit`; JSON text payload | ReadOnly | Safe |
| `write_file` | `run_write_file` | `path`, `content`; JSON write/patch result | WorkspaceWrite | Safe if scoped |
| `edit_file` | `run_edit_file` | `path`, `old_string`, `new_string`, `replace_all`; JSON edit result | WorkspaceWrite | Safe if scoped |
| `glob_search` | `run_glob_search` | `pattern`, optional `path`; filenames/count/duration | ReadOnly | Safe |
| `grep_search` | `run_grep_search` | regex, path/glob/context/output filters; search result | ReadOnly | Safe |
| `NotebookEdit` | `run_notebook_edit` | notebook path, cell id/source/type/edit mode | WorkspaceWrite | Safe if scoped |

Native move/copy/delete/watch tools: **NOT FOUND IN REPOSITORY.**

## B. Shell/Command Execution

| Tool | Function | Input/output | Permission | Air-gap |
|---|---|---|---|---|
| `bash` | `run_bash` | command, timeout, background, sandbox/network/filesystem options; stdout/stderr/status/sandbox status | DangerFullAccess | Unsafe by default |
| `PowerShell` | `run_powershell` | command, timeout, background; process output | DangerFullAccess | Unsafe by default |
| `REPL` | `run_repl` | language, code, timeout; captured process output | DangerFullAccess | Unsafe by default |
| `Sleep` | `run_sleep` | duration | ReadOnly | Safe |

`REPL` supports installed runtime programs selected by language. It does not provide Python package isolation.

## C. Coding and Agent Coordination

`Agent`, `TodoWrite`, `TaskCreate`, `RunTaskPacket`, `TaskGet`, `TaskList`, `TaskStop`, `TaskUpdate`, `TaskOutput`, `WorkerCreate`, `WorkerGet`, `WorkerObserve`, `WorkerResolveTrust`, `WorkerAwaitReady`, `WorkerSendPrompt`, `WorkerRestart`, `WorkerTerminate`, `WorkerObserveCompletion`, `TeamCreate`, and `TeamDelete` are implemented in `rust/crates/tools/src/lib.rs` and connect to runtime task/worker modules.

They provide model-directed delegation and persisted local state, but not a formal fault-tolerant workflow engine.

## D. Web/Network

`WebFetch`, `WebSearch`, `RemoteTrigger`, `MCP`, `McpAuth`, `ListMcpResources`, and `ReadMcpResource` are registered in `rust/crates/tools/src/lib.rs`.

They are not safe for an air-gapped deployment unless disabled by policy. `MCP` is only air-gap-safe when limited to local stdio/loopback servers.

## E. Git

`GitStatus`, `GitDiff`, `GitLog`, `GitShow`, and `GitBlame` are implemented in the tools crate and are read-oriented. Arbitrary Git network operations remain possible through `bash`.

## F-I. Documents, Data, Images, Other

- PDF extraction: partial, `rust/crates/tools/src/pdf_extract.rs`.
- StructuredOutput, Config, plan-mode, LSP, cron, user-question, messaging, Skill, and ToolSearch: implemented.
- DOCX/XLSX/PPTX/OCR/image/VLM/image-generation tools: **NOT FOUND IN REPOSITORY.**

# Existing Skills

Skill discovery uses Markdown roots and frontmatter as described in `rust/crates/tools/src/lib.rs` and `rust/crates/commands/src/lib.rs`.

The repository contains no production `SKILL.md` files or ordinary application skills. Test fixtures and harness directories exist, but no MRPL workflow skills are shipped.

The requested `document-analysis`, `knowledge-search`, `approval-note`, `engineering-analysis`, `coding-agent`, `spreadsheet-analysis`, `multimodal-analysis`, `security-policy`, and `artifact-generation` skills can be created without core changes if they orchestrate built-in or MCP tools.

`model-routing` can describe policy, but cannot change the provider for the current turn without core/provider changes.

# MCP Support

MCP client-side types exist in:

- `rust/crates/runtime/src/mcp_client.rs`
- `rust/crates/runtime/src/mcp_stdio.rs`
- `rust/crates/runtime/src/mcp_tool_bridge.rs`

The code models Stdio, SSE, HTTP, WebSocket, SDK, and managed-proxy transports. The actual `McpServerManager` currently registers and runs only stdio servers:

`rust/crates/runtime/src/mcp_stdio.rs:495`

Implemented stdio lifecycle:

- Process spawning
- JSON-RPC framing
- `initialize`
- `tools/list`
- `tools/call`
- `resources/list`
- `resources/read`
- Timeouts
- Retry/reset
- Partial/degraded startup reports
- Multiple configured servers
- Qualified names such as `mcp__server__tool`

Crudo also contains a local stdio MCP server in `rust/crates/runtime/src/mcp_server.rs`, supporting `initialize`, `tools/list`, and `tools/call`.

Configuration is parsed from `mcpServers` in `rust/crates/runtime/src/config.rs:1561`.

Example:

```json
{
  "mcpServers": {
    "docling": {
      "type": "stdio",
      "command": "/opt/crudo/bin/docling-mcp",
      "args": ["--stdio"],
      "env": { "DOCS_ROOT": "/knowledge" },
      "toolCallTimeoutMs": 60000,
      "required": true
    }
  }
}
```

Remote MCP configuration types exist, but the current manager marks non-stdio transports unsupported. Prompts support is **NOT FOUND IN REPOSITORY**.

# Model Support

Verified provider families:

- Anthropic
- xAI
- OpenAI
- DashScope/Qwen through OpenAI-compatible API
- Ollama through OpenAI-compatible API
- Arbitrary OpenAI-compatible endpoints through configured base URLs

Relevant files:

- `rust/crates/api/src/providers/mod.rs:121`
- `rust/crates/api/src/client.rs:20`
- `rust/crates/api/src/providers/openai_compat.rs`

Ollama is explicitly supported through `OLLAMA_HOST`; local authless OpenAI-compatible endpoints are tested.

llama.cpp and vLLM are not named explicitly. They can work only if their server exposes a compatible Chat Completions API. This is an architectural compatibility inference, not a repository-verified integration.

Multiple provider credentials, explicit model-prefix routing, ordered provider fallbacks, and explicit subagent models exist. Task-type routing, automatic vision/coding/embedding selection, and per-tool model selection do not.

The smallest router addition is a new `api/src/router.rs`, integrated into `api/src/client.rs`, CLI model construction, subagent model resolution, and `runtime/src/config.rs`.

# Agent Capabilities

| Capability | Status |
|---|---|
| Create plans | Partial: plan mode and todo/task tools |
| Break into subtasks | Implemented through Agent/tasks/workers |
| Call multiple tools | Implemented |
| Observe results | Implemented through tool results |
| Modify plan | Model can update todos/plan state |
| Retry | API retries and some MCP/task retry mechanisms; no universal workflow retry engine |
| Recover from errors | Partial |
| Run tests | Via `bash`/`REPL`; no dedicated test abstraction |
| Iterate/debug | Implemented through repeated tool loop |
| Conditional/sequential calls | Implemented, model-directed |
| Parallel calls | No clear parallel execution in `ConversationRuntime` |
| Delegate to subagents | Implemented |
| Maintain state | Sessions, task files, manifests, workers |

Current request trace:

```text
User prompt
  -> LiveCli
  -> ProviderClient.stream
  -> ConversationRuntime::run_turn
  -> model emits tool_use
  -> PreToolUse hook
  -> PermissionPolicy
  -> ToolExecutor::execute
  -> PostToolUse hook
  -> tool_result appended to Session
  -> model called again
  -> final text
  -> session/usage persisted
```

There is no separate planner process between `LiveCli` and the model.

# Document Capabilities

| Format | Status |
|---|---|
| TXT | Text file tool |
| Markdown | Text file tool and prompt/context files |
| HTML | RAG service text extension; not rich document parsing |
| CSV | Text file only |
| PDF | Minimal text extraction only |
| Scanned PDF | Unsupported |
| DOCX | Unsupported |
| XLSX | Unsupported |
| PPTX | Unsupported |
| Images | Unsupported as model inputs |
| Binary files | Built-in text reader rejects them |

PDF extraction is implemented in `rust/crates/tools/src/pdf_extract.rs:15`. It handles common PDF text operators but does not render pages, OCR scans, or reliably understand layout and complex fonts.

`InputContentBlock` supports text, thinking, tool use, and tool results, but no image block: `rust/crates/api/src/types.rs:88`.

Therefore image input, vision calls, image attachments, PDF rendering, and image tool results are not natively implemented.

Docling can be integrated cleanly as a local stdio MCP server exposing a document parsing tool.

# Multimodal Capabilities

Current native multimodal capability: **none verified**.

Recommended local pipeline:

```text
PDF/image
  -> PDF renderer
  -> OCR/VLM
  -> structured Markdown/JSON
  -> Crudo text tool or MCP result
```

PaddleOCR, Tesseract, Docling, PaddleOCR-VL, and Qwen-VL are not repository dependencies. They must be installed in the deployment image or exposed by local MCP services.

True native image reasoning requires modifying `InputContentBlock`, provider serialization, stream/result conversion, tool-result content handling, provider capabilities, and the runtime attachment model.

# RAG Capabilities

A separate service exists in `rust/crates/crudo-rag-service/src`.

Implemented:

- Workspace walking
- Text-extension filtering
- Chunking with overlap
- SQLite metadata/chunks/embeddings
- OpenAI-compatible embedding endpoint
- Mock embeddings for tests
- Cosine similarity
- Linear-scan retrieval
- Optional Qdrant indexing/query
- HTTP `/v1/query`
- HTTP `/v1/stats`

Important files:

- `crudo-rag-service/src/ingest.rs`
- `crudo-rag-service/src/search.rs`
- `crudo-rag-service/src/embed.rs`
- `crudo-rag-service/src/qdrant_index.rs`

Limitations:

- Default embedding URL is `https://api.openai.com/v1`.
- Local embeddings require a local compatible endpoint.
- Ingestion is text-only.
- PDF/DOCX/XLSX/PPTX/OCR/Docling are not integrated.
- Reranking is absent.
- Hybrid keyword/semantic search is absent.
- Full metadata filtering is absent.
- Citations are path/snippet results, not robust page/region citations.
- The main agent does not automatically call `/v1/query`.

The requested pipeline is only partially present. A new built-in retrieval tool or local MCP wrapper is required to connect the RAG service to Crudo.

Qdrant is the only requested vector database with repository integration. Chroma, FAISS, and Milvus are **NOT FOUND IN REPOSITORY**.

# Sandbox Capabilities

The sandbox model is implemented in `rust/crates/runtime/src/sandbox.rs` and used by `rust/crates/runtime/src/bash.rs:281`.

It supports enabled/disabled state, Linux namespace restrictions, network isolation, filesystem modes, allowed mounts, container detection, `unshare` launcher construction, and fallback status reporting.

If Linux namespace isolation is unavailable, command execution falls back to host `sh -lc`. There is no verified Docker, Podman, gVisor, or Firecracker integration; no cgroup CPU/memory/PID limits; no seccomp policy; and no guarantee of read-only host filesystem.

For production, use a rootless container or VM with no network route, read-only base image, writable `/workspace` and `/output`, read-only `/knowledge`, resource limits, non-root user, seccomp, no host sockets, no credentials, and explicit command allowlists.

# Artifact Capabilities

Native generation is limited to source/text files through `write_file` and `edit_file`.

DOCX, XLSX, PPTX, PDF generation, image generation, rendering, verification, and general artifact-return protocols are **NOT FOUND IN REPOSITORY**.

An artifact MCP server can expose:

```text
create_docx
create_xlsx
create_pptx
render_document
verify_document
```

Recommended implementation uses local office libraries and LibreOffice headless for rendering and verification.

# Security/Air-Gap Analysis

Potential external paths include model API clients, `WebFetch`, `WebSearch`, `RemoteTrigger`, remote MCP configuration, OAuth, RAG embeddings, Qdrant URLs, arbitrary Git through `bash`, and telemetry/analytics paths.

Relevant locations include:

- `rust/crates/api/src/providers/anthropic.rs`
- `rust/crates/api/src/providers/openai_compat.rs`
- `rust/crates/tools/src/lib.rs`
- `rust/crates/runtime/src/mcp_client.rs`
- `rust/crates/crudo-rag-service/src/embed.rs`
- `rust/crates/crudo-rag-service/src/qdrant_index.rs`
- `rust/crates/telemetry/src/lib.rs`

Crudo can operate without network if all models, dependencies, embeddings, MCP servers, and databases are local and external tools are disabled. It does not centrally enforce that condition.

Required controls:

- Central network policy allowing loopback only
- Disable WebFetch/WebSearch/RemoteTrigger
- Disable remote MCP
- Restrict RAG/Qdrant/embedding URLs to loopback
- Restrict Git network operations
- Filter subprocess environment variables
- Fail closed when sandbox isolation is unavailable
- Add immutable tool, file, model, and network audit events

# Logging / Observability

Existing observability includes:

- Structured CLI JSON errors
- Session usage and token accounting
- Prompt cache events
- `SessionTracer`
- Assistant iteration events
- Tool start/finish events
- Turn start/completion/failure events
- MCP degraded reports
- Sandbox status and fallback reasons
- Some lane events and task manifests

Relevant files:

- `rust/crates/runtime/src/conversation.rs`
- `rust/crates/runtime/src/usage.rs`
- `rust/crates/telemetry/src/lib.rs`
- `rust/crates/runtime/src/mcp_lifecycle_hardened.rs`
- `rust/crates/runtime/src/report_schema.rs`

Not consistently available as a complete audit record:

- Full tool inputs and outputs
- Execution duration per tool
- Every file accessed
- Every network attempt
- Artifact checksums and verification lineage
- Selected routing decision and reason

A Sovereignty Dashboard is therefore a medium/high-complexity addition. It should consume structured events rather than scrape terminal output.

# Configuration

Main configuration files:

- `~/.crudo/settings.json`
- `.crudo.json`
- `.crudo/settings.json`
- `.crudo/settings.local.json`

Examples of supported settings:

```json
{
  "model": "local-reasoner",
  "provider": {
    "kind": "openai-compatible",
    "baseUrl": "http://127.0.0.1:8000/v1"
  },
  "permissions": {
    "defaultMode": "workspace-write"
  },
  "sandbox": {
    "enabled": true,
    "namespaceRestrictions": true,
    "networkIsolation": true,
    "filesystemMode": "allow-list",
    "allowedMounts": ["/workspace", "/knowledge", "/output"]
  },
  "mcpServers": {}
}
```

Environment variables include `CRUDO_CONFIG_HOME`, `OLLAMA_HOST`, provider API keys/base URLs, RAG variables, and sandbox-related options.

`LOCAL_MODE`, `NETWORK_ACCESS`, and `MODEL_ROUTER` are not current first-class settings. They should be added to `RuntimeFeatureConfig` and validated by `ConfigLoader`.

# Plugin Architecture

The easiest extension path is Markdown skills. Next is local stdio MCP. Then plugins with tools/hooks/commands. Built-in Rust tools require modifying the tools registry and dispatch. Model providers, routing, native multimodality, and OS-grade security require core changes.

| Extension | Without core changes? |
|---|---:|
| Skills | Yes |
| Local MCP tools | Yes |
| Plugin tools | Yes |
| Hooks | Yes, within existing hook model |
| Commands | Plugin-supported, subject to existing command model |
| Model provider | No general provider ABI; core change recommended |
| Model router | No, unless external proxy is used |
| Native multimodal | No |
| Strong sandbox | External sidecar possible; core integration recommended |

# Performance / Hardware

Crudo itself is a CLI/runtime and does not impose model VRAM requirements. It communicates with model servers through HTTP-compatible APIs, so model size and quantization are determined by the external serving process.

The repository does not contain model benchmarks or hardware profiles. It cannot support a repository-grounded recommendation for exact model sizes on 6/8/12/16/24/48GB GPUs.

Architecturally it can consume any model server that exposes the supported protocol and fits the host hardware. Quantized local models are practical only insofar as the selected external server supports their format.

# What We Can Build Without Core Changes

| Capability | Existing? | Skill | MCP | Core modification? |
|---|---:|---:|---:|---:|
| OCR | No | Workflow only | Yes | No |
| PDF parsing | Minimal text only | Workflow only | Yes | No |
| Docling | No | Workflow only | Yes | No |
| Qdrant | Optional RAG feature | Workflow only | Yes | No |
| RAG retrieval | Standalone service only | Yes | Yes | No, if MCP |
| Embeddings | RAG service | Workflow only | Yes | No |
| Reranking | No | Workflow only | Yes | No |
| Word generation | No | Workflow only | Yes | No |
| Excel | No | Workflow only | Yes | No |
| PowerPoint | No | Workflow only | Yes | No |
| Sandbox | Partial | Policy instructions only | Yes | Yes for strong isolation |
| gVisor | No | No | Sidecar wrapper | No for external launcher |
| Vision | No native message support | Workflow only | VLM MCP possible | Yes for native images |
| Model router | No | Policy only | External proxy possible | Yes for integrated routing |
| Security policy | Partial | Yes | Yes | Yes for fail-closed enforcement |
| Network monitoring | No | No | Sidecar possible | Core audit integration recommended |
| Engineering calculations | Via REPL/bash | Yes | Yes | No |
| Approval-note workflow | No | Yes | Yes | No for external workflow |

# Recommended Architecture

```text
                         CRUDO CLI
                             |
             +---------------+----------------+
             |                                |
       Model Router                    ConversationRuntime
             |                                |
    +--------+--------+                       |
    |        |        |                       |
 reasoning coding  vision              ToolExecutor
    |        |        |                       |
 ProviderClient  local VLM                  |
                                             |
                              +--------------+-------------+
                              |              |             |
                         stdio MCP       Plugins       built-ins
                              |
       +----------+----------+----------+------------+
       |          |          |          |            |
    Docling      OCR       Qdrant     Sandbox       Office
```

Actual integration points:

| Block | Location |
|---|---|
| CLI | `rust/crates/crudo-cli/src/main.rs` |
| Agent loop | `rust/crates/runtime/src/conversation.rs::ConversationRuntime::run_turn` |
| Models | `rust/crates/api/src/client.rs::ProviderClient` |
| Providers | `rust/crates/api/src/providers/mod.rs::Provider` |
| Tools | `rust/crates/tools/src/lib.rs::ToolSpec`, `execute_tool` |
| MCP | `rust/crates/runtime/src/mcp_stdio.rs::McpServerManager` |
| Plugins | `rust/crates/plugins/src/lib.rs::PluginManager` |
| Skills | `rust/crates/tools/src/lib.rs` skill resolver |
| Hooks | `rust/crates/runtime/src/hooks.rs::HookRunner` |
| Permissions | `rust/crates/runtime/src/permission_enforcer.rs::PermissionEnforcer` |
| Filesystem | `rust/crates/runtime/src/file_ops.rs` |
| Shell | `rust/crates/runtime/src/bash.rs` |
| Sandbox | `rust/crates/runtime/src/sandbox.rs` |
| RAG | `rust/crates/crudo-rag-service/src` |
| Sessions | `rust/crates/runtime/src/session.rs`, `session_control.rs` |

# Minimum Viable Demo

The smallest credible judge-facing demo should use a local OpenAI-compatible server, two local models, and local MCP sidecars.

Preparation:

1. Start local reasoning model endpoint.
2. Start local coding model endpoint.
3. Start local vision model endpoint or VLM MCP service.
4. Start local embedding endpoint.
5. Start local RAG service bound to `127.0.0.1`.
6. Start local Docling/OCR MCP server.
7. Start local Office artifact MCP server.
8. Start sandbox launcher with network disabled.
9. Configure Crudo with only loopback endpoints and local MCP servers.

Demonstration:

```text
USER: Upload inspection_report.pdf

CRUDO:
- Calls local Docling/OCR MCP.
- Receives structured Markdown/JSON.
- Stores extracted text locally.

USER: Prepare an approval note using the inspection report and applicable SOPs.

CRUDO:
- Selects the reasoning model.
- Calls local retrieval MCP.
- Searches Qdrant/SQLite for SOPs.
- Extracts findings and compares them to procedures.
- Generates a DOCX through local artifact MCP.
- Renders and verifies the DOCX.

USER: Write a Python calculation that checks the pressure-drop values.

CRUDO:
- Selects the coding model.
- Writes code to /workspace.
- Executes it in the sandbox.
- Runs tests, repairs failures, and reruns them.

USER: Analyze pid_photo.png and identify the tagged isolation valves.

CRUDO:
- Selects the vision model.
- Calls local VLM/OCR MCP.
- Cross-checks tags against the local knowledge base.
- Adds findings to the approval note.

CRUDO:
- Displays local model endpoints.
- Displays local MCP transports.
- Displays network policy: deny non-loopback.
- Displays tool-call audit events.
```

This demo cannot be delivered using only current built-in capabilities because native vision, OCR, office generation, integrated retrieval, and strong sandboxing are missing. It can be delivered with local sidecar MCP services plus limited core changes.

# Full Demo

A full industrial workflow should add project and organization knowledge roots, incremental ingestion, OCR confidence, layout-aware extraction, page/region citations, VLM P&ID interpretation, model routing, approval gates, human approval, office rendering, spreadsheet analysis, calculation provenance, test/artifact verification, audit export, and replayable workflow state.

# Implementation Roadmap

## PHASE 0 — Understand and Prepare Crudo

Use `ConfigLoader`, permissions, sessions, MCP, and plugins. Add deployment/config templates for `/workspace`, `/knowledge`, `/output`, loopback-only endpoints, and disabled network tools. Complexity: LOW.

## PHASE 1 — Local Model Serving

Reuse `ProviderClient`, `OpenAiCompatClient`, `OLLAMA_HOST`, and `OPENAI_BASE_URL`. Add local compatible model servers. Complexity: LOW/MEDIUM. Satisfies local inference and multiple models.

## PHASE 2 — MCP/Tools

Reuse `McpServerManager`, `McpToolRegistry`, and stdio JSON-RPC. Add local MCP wrappers for document, RAG, OCR, and artifacts. Complexity: MEDIUM.

## PHASE 3 — Document/OCR

Extend sidecars or `crudo-rag-service/src/ingest.rs`; reuse `pdf_extract.rs` and stdio MCP. Add Docling, OCR, and PDF rendering. Complexity: HIGH.

## PHASE 4 — RAG

Extend `ingest.rs`, `search.rs`, `embed.rs`, and `qdrant_index.rs`; expose retrieval through MCP. Add local embeddings and reranking. Complexity: MEDIUM/HIGH.

## PHASE 5 — Sandbox

Extend `runtime/src/sandbox.rs` and `bash.rs`, preferably with a dedicated container/VM launcher. Add resource controls, no network, non-root execution, and fail-closed behavior. Complexity: HIGH.

## PHASE 6 — Artifacts

Add an artifact MCP server using office libraries and LibreOffice headless. Reuse file tools and MCP/plugin registration. Complexity: MEDIUM/HIGH.

## PHASE 7 — Model Routing

Create `api/src/router.rs`; modify `api/src/client.rs`, CLI construction, subagent model resolution, and `runtime/src/config.rs`. Start with deterministic rules. Complexity: MEDIUM.

## PHASE 8 — Multimodal

Modify `api/src/types.rs`, provider serializers, runtime attachment handling, and tool-result content. Add a local VLM. Complexity: HIGH.

## PHASE 9 — Security/Air-Gap

Add central network policy and integrate it with API, tools, MCP, RAG, Qdrant, Git, sandbox, and telemetry. Complexity: HIGH.

## PHASE 10 — Demo UI

Create a dashboard over structured session/tool/audit events. Reuse JSON output contracts and existing RAG static UI patterns. Complexity: MEDIUM.

# Risks / Blockers

1. Sandbox fallback is not fail-closed.
2. Native multimodal support is absent.
3. RAG is not integrated with the main agent.
4. RAG embeddings default to an external OpenAI URL.
5. Remote MCP support is incomplete.
6. Office artifact generation is absent.
7. No central network-deny policy exists.
8. Tool logging is not a complete immutable audit record.
9. Parallel execution is limited.
10. No dedicated artifact/workflow verification engine exists.
11. Provider routing is model-family based, not task based.
12. The repository worktree is dirty and includes the ongoing Crudo rename.

# Final Readiness Score

## Can Crudo realistically become the target system?

**B. PARTIALLY — major extensions are required, but the architecture does not fundamentally prevent it.**

The existing core is a credible agent runtime and is well suited to orchestration. It already has sessions, model calls, tools, permissions, hooks, plugins, MCP, subagents, and structured outputs.

The target workbench still requires industrial document processing, OCR, scanned-PDF understanding, native multimodal inputs, integrated RAG, artifact generation/verification, strong sandboxing, central air-gap enforcement, task-aware routing, and complete security observability.

These should be added primarily as local MCP/sidecar services, with focused core modifications for routing, multimodal messages, security policy, retrieval integration, and audit events.

| State | Score |
|---|---:|
| Current Crudo readiness | **5/10** |
| After minimal extensions | **7/10** |
| After full implementation | **9/10** |

Fastest viable path:

```text
Current Crudo
  + local OpenAI-compatible models
  + local stdio MCP services
  + Docling/OCR
  + Qdrant/RAG
  + artifact service
  + container/VM sandbox
  + deterministic router
  + network-deny policy
```
