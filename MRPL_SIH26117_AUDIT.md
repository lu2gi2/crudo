# MRPL SIH26117 Post-Implementation Repository Audit

**Repository:** Crudo (`feature/industrial-demo-tools`)
**Audited commit:** `98e7260`
**Audit date:** 2026-09-07
**Scope:** Current repository state versus MRPL SIH26117, “Sovereign On-Premise Agentic AI Workbench using Open-Weight Multimodal LLMs for Confidential Industrial Work.”

> This audit distinguishes code existence from actual integration and test evidence. The repository contains synthetic MRPL-themed demo data, but no actual SIH26117 specification package, MRPL corpus, operational refinery documents, P&IDs, or production deployment evidence.

## 1. Executive Summary

| Measure | Assessment |
|---|---:|
| Overall engineering readiness | **5.0 / 10** |
| Estimated SIH26117 readiness | **35–40%** |
| Core runtime maturity | Strong |
| Industrial demo maturity | Prototype |
| Sovereignty/security readiness | Weak |
| Current verification confidence | Low to moderate |

### Material improvements since the previous audit

- Added a local industrial-demo stdio MCP server in `demo/mcp_server/server.py`.
- Added optional PaddleOCR integration with structured page/text/confidence output.
- Added synthetic document extraction and local keyword knowledge search.
- Added PPTX generation and DOCX approval-note generation.
- Added LibreOffice temporary PDF rendering checks in `demo/mcp_server/office.py`.
- Added basic presentation and Word-document verification checks.
- Added SHA-256 output hashing for generated approval notes.
- Added demo artifacts and a board-presentation workflow skill.
- Confirmed the Rust CLI actually discovers and calls configured MCP tools through `RuntimeMcpState` and `McpServerManager`.

### Completely missing

- Native image blocks and multimodal model input.
- Local VLM integration.
- P&ID and engineering-drawing understanding.
- Handwritten-note understanding.
- Multimodal retrieval and image-region citations.
- Automatic RAG use in the main agent loop.
- Task-aware model routing.
- XLSX generation and verification.
- Strong fail-closed sandboxing.
- Central network-deny enforcement.
- Complete evidence/artifact provenance.
- Production MRPL corpus and workflow tests.
- SIH26117 traceability and acceptance matrix.

### Partially implemented

- PDF processing: minimal text extraction only.
- OCR: optional demo-sidecar integration, not proven operational.
- RAG: standalone text-only service, not integrated into the normal agent loop.
- Qdrant: optional integration, not proven end to end.
- MCP: local stdio is real; other transports are modeled but unsupported by the manager.
- Sandbox: best-effort namespace isolation with host-shell fallback.
- Permissions: meaningful policy exists, but some prompt-mode paths return `Allowed` without an approval result.
- Artifacts: PPTX/DOCX demo generation exists, but verification is text-contract based and currently has a stale fixture.
- Provenance: session/tool events exist, but full source-to-artifact lineage is absent.

### Genuinely demo-ready

- Rust CLI agent loop and streaming model interaction.
- Local stdio MCP lifecycle.
- Synthetic document-to-approval-note workflow.
- Synthetic plan-to-PPTX workflow.
- Local keyword search over demo Markdown.
- Basic PPTX/DOCX structural checks.
- Synthetic-data and human-review labeling.
- Session persistence and iterative tool calls.

These capabilities are not production-ready for confidential industrial use because network, shell, sandbox, secret-handling, provenance, and parser boundaries are not sufficiently enforced.

### Five biggest blockers

1. Fail-open execution and network security.
2. No integrated multimodal industrial document pipeline.
3. RAG is not connected to ordinary agent reasoning.
4. No deterministic task/modality model router.
5. No real MRPL corpus or complete end-to-end evidence.

### Five strongest aspects

1. Real multi-step model/tool agent runtime.
2. Extensible local stdio MCP architecture.
3. Broad runtime infrastructure: sessions, compaction, hooks, plugins, subagents, and task state.
4. Explicit synthetic-data and human-review labeling.
5. Directionally correct Office artifact rendering checks.

### Blunt jury assessment

The runtime and local-MCP architecture would impress a technical jury. The synthetic artifact workflow is a useful proof of direction. The project would be downgraded or rejected if it claimed to already be a sovereign industrial workbench, because the current repository does not prove offline execution, local VLM capability, integrated MRPL RAG, P&ID understanding, fail-closed isolation, or production-grade provenance.

The architecture remains suitable as an orchestration core, but requires focused additions rather than a rewrite.

## 2. Current Changes Since the Previous Audit

Recent commits include:

```text
d76b80f feat: add industrial demo integration
d4b78b7 fix: improve presentation tool workflow
ba483ce feat: add document and approval note tools
8fe1886 feat: rebrand interactive banner as Crudo
98e7260 feat: expand demo document verification
```

The latest implementation added or changed:

- `demo/mcp_server/office.py`
- `demo/tests/test_office_verification.py`
- `demo/data/indian_literacy_rate_plan.json`
- `demo/data/inspection_page.png`
- `demo/output/indian_literacy_rate_presentation.pptx`
- `demo/output/presentation_demo_workflow_deck.pptx`
- `demo/mcp_server/server.py`

Unrelated educational/profile files were also added:

- `dynamic-programming.html`
- `quantum-computing.html`
- `syed-muhammed.html`

These do not materially improve SIH26117 readiness.

## 3. Requirement-by-Requirement Audit

### Sovereignty

| Requirement | Status | Evidence | Integration/test | Gap | Priority |
|---|---|---|---|---|---|
| Fully on-premise deployment | Possible, not proven | Provider abstraction in `rust/crates/api` | No sovereign deployment profile | No offline deployment contract | P0 |
| No cloud dependency | Not guaranteed | RAG defaults to `https://api.openai.com/v1` in `rust/crates/crudo-rag-service/src/embed.rs:20-23` | No clean-room test | Must require local endpoint or fail closed | P0 |
| No external API dependency | Not guaranteed | Anthropic/OpenAI-compatible clients and web tools exist | Configuration-dependent | No central allowlist | P0 |
| Local model inference | Supported by endpoint | OpenAI-compatible provider/Ollama configuration | Model server external to repo | No bundled or verified model run | P1 |
| Local embeddings | Mock or external | `embed.rs:62-100` | Mock mode exists | No proven local embedding deployment | P0 |
| Local vector database | SQLite functional; Qdrant optional | RAG `db.rs`, `search.rs`, `qdrant_index.rs` | Standalone service | Qdrant path not proven end to end | P1 |
| Local document processing | Demo-sidecar only | Docling import in `server.py:72-96` | Optional MCP | Dependencies not pinned/bundled | P1 |
| Local OCR | Optional prototype | PaddleOCR in `server.py:97-126` | No passing OCR test | Model/dependency availability unknown | P0 |
| Local VLM | Missing | `analyze_image` returns unavailable at `server.py:202-203` | None | Add local VLM or VLM MCP | P0 |
| Local artifact generation | Demo prototype | `presentation.py`, `server.py:135-190` | Partial | No XLSX/PDF/report service | P1 |
| Local MCP services | Implemented | Rust stdio manager plus Python demo server | Integrated via `.crudo/settings.local.json` | Arbitrary process and host access risks | P1 |

### Confidential industrial data

| Data | Status | Assessment |
|---|---|---|
| P&IDs | Missing | No parser, VLM, or region citation path |
| Engineering drawings | Missing | No image/layout engineering pipeline |
| Inspection reports | Synthetic demo only | `demo/data/inspection_report.md` is not an operational document |
| Scanned PDFs | Missing/partial | Optional OCR only; no reliable scanned workflow |
| Handwritten notes | Missing | No handwriting model or test |
| Internal correspondence | Text-only possible | No confidential-data policy |
| Financial/vendor/strategy documents | Text-only possible | No field-level access or redaction model |

### Agentic behavior

| Capability | Status | Evidence |
|---|---|---|
| Planning | Partial | Plan mode and task tools; model-directed |
| Task decomposition | Implemented | Agent/task/worker infrastructure |
| Tool selection | Implemented | Registry plus model tool calls |
| Multi-step execution | Implemented | `ConversationRuntime::run_turn` |
| Tool-result handling | Implemented | Results appended to session |
| Retries | Partial | Provider/MCP retry paths; no universal policy |
| Error recovery | Partial | Some retry/reset/task recovery |
| Subagents | Implemented | Worker/subagent modules |
| State persistence | Implemented | Sessions, tasks, manifests |
| Human approval | Partial | Permission prompts/tokens, not domain sign-off |
| Verification | Partial | Demo artifact checks, not engineering verification |
| Parallel execution | Limited | No clear general parallel tool execution |

### Multimodal capability

| Capability | Status |
|---|---|
| Image input to model | Missing natively |
| PDF image understanding | Missing |
| Scanned document understanding | Optional OCR attempt only |
| VLM | Missing |
| P&ID understanding | Missing |
| Diagram semantics | Missing |
| Handwriting | Missing |
| Image-to-structured-data | Partial OCR prototype |
| Multimodal retrieval | Missing |
| Image/page-region citations | Missing |

### Model routing

Routing currently uses provider/model aliases, prefixes, explicit model selection, and configured fallback chains. Relevant code includes:

- `rust/crates/api/src/client.rs`
- `rust/crates/api/src/providers/mod.rs`
- `rust/crates/tools/src/lib.rs:5121-5173`
- `rust/crates/runtime/src/config.rs:1034-1050`

There is no verified router that automatically selects reasoning, coding, vision, OCR, embedding, extraction, or summarization models by task. Routing is primarily provider/model configuration, not task-aware routing.

### RAG / knowledge system

The standalone RAG path is:

```text
workspace files
  -> WalkDir scan
  -> text-extension filter
  -> hash/metadata
  -> fixed-size chunks
  -> HTTP or mock embeddings
  -> SQLite vectors
  -> optional Qdrant
  -> cosine similarity
  -> /v1/query
```

Relevant files:

- `rust/crates/crudo-rag-service/src/ingest.rs`
- `rust/crates/crudo-rag-service/src/chunk.rs`
- `rust/crates/crudo-rag-service/src/embed.rs`
- `rust/crates/crudo-rag-service/src/search.rs`
- `rust/crates/crudo-rag-service/src/main.rs`

Implemented: text ingestion, chunking, embeddings, SQLite storage, optional Qdrant, cosine retrieval, incremental updates, deletion of removed files.

Missing: document-format ingestion, OCR integration, reranking, hybrid search, robust filtering, page/region citations, and normal-agent integration.

The industrial demo's `search_knowledge` tool at `server.py:127-134` performs keyword matching over local Markdown. It is not semantic MRPL RAG.

**Can the agent automatically retrieve MRPL knowledge during a task? No.** The service is standalone, and no ordinary `ConversationRuntime` path automatically invokes `/v1/query`.

### Industrial document processing

| Format | Status |
|---|---|
| TXT/Markdown | Implemented |
| HTML/CSV | Text-only |
| PDF | Minimal text extraction |
| Scanned PDF | Unsupported reliably |
| DOCX | Demo generation/reading only |
| XLSX | Missing |
| PPTX | Demo generation/read/verify |
| Images | OCR fixture only |
| Tables/layout/page refs | Not generally supported |
| Engineering drawings | Missing |

### Artifact generation

PPTX and DOCX generation exists in the demo sidecar. DOCX approval notes include synthetic-data and human-review notices and receive a SHA-256 digest. LibreOffice is used for temporary PDF rendering.

This does not prove:

- engineering value correctness,
- unit/formula correctness,
- source-page mapping,
- visual correctness,
- safe recommendations,
- approval authority,
- complete tamper/provenance lineage.

XLSX generation is missing. Durable PDF output and a general artifact registry are missing.

### Code execution and sandbox

Execution paths include shell, REPL, PowerShell, workers, hooks, plugins, and MCP subprocesses.

Sandbox status is calculated in `rust/crates/runtime/src/sandbox.rs:155-206`, and an `unshare` command is built at `211-260`. However, when no launcher is available, `rust/crates/runtime/src/bash.rs:306-320` and `334-347` fall back to plain `sh -lc`.

| Control | Status |
|---|---|
| Namespace isolation | Best effort |
| Network namespace | Best effort |
| Filesystem isolation | Not actually enforced |
| CPU/memory/PID limits | Missing |
| seccomp | Missing |
| Container/VM boundary | Missing in core |
| Timeout | Partial |
| Network default | `false` in `sandbox.rs:99` |
| Sandbox failure | Fails open to host shell |
| Environment filtering | Incomplete |

## 4. Air-Gap and Network Attack Surface

| Component | Network capability | Default/bypass | Mitigation status |
|---|---|---|---|
| Anthropic/OpenAI-compatible providers | Yes | Provider/base URL configuration | No central allowlist |
| RAG embeddings | Yes | Defaults to OpenAI URL | Mock mode is optional |
| Qdrant | Yes | Compose publishes ports | No auth/bind restriction |
| RAG HTTP service | Yes | `0.0.0.0:8787` in Compose | No auth |
| WebFetch/WebSearch | Yes | Core tools exist | Must be disabled in sovereign mode |
| RemoteTrigger | Potentially yes | Core tool exists | Must be disabled |
| Git via shell | Yes | Arbitrary Bash | No network command policy |
| Hooks/plugins/MCP | Indirectly yes | Arbitrary subprocesses | No guaranteed isolation |
| Installer | Yes | Remote endpoint supported/defaulted | Development warning only |
| Container builds | Yes | apt/cargo/floating images | No offline mirror contract |

### Fail-open behavior

- Sandbox unavailable: host-shell fallback.
- Network namespace unavailable: fallback status, execution may continue.
- Filesystem isolation: environment setup rather than enforcement.
- MCP failures: best-effort degraded startup.
- Prompt-mode permission checks: `Allowed` can be returned without a prompter.
- Malformed configuration: warnings may not block loading.
- Missing embedding credentials: that service fails unless mock mode is used.

The repository can be configured for local operation, but it does not centrally prove or enforce local-only operation.

## 5. Clean-Room Offline Assessment

| Operation | Current result in an Internet/DNS-disabled environment |
|---|---|
| Start core | Likely, if dependencies are already installed |
| Local model | Yes only if separately deployed |
| Mock RAG | Possible with `CRUDO_RAG_MOCK_PROVIDERS=1` |
| Real local embeddings | Only with a local compatible embedding endpoint |
| Default RAG | Fails or attempts external OpenAI endpoint |
| OCR | Only if PaddleOCR and model assets are preinstalled |
| VLM | Not supported |
| Office generation | Possible if Python packages and LibreOffice exist |
| Local MCP | Possible, dependencies permitting |
| Complete MRPL workflow | Not supported |

Hidden dependencies include Rust/package downloads, floating Qdrant/container images, Docling/PaddleOCR model assets, Python Office libraries, LibreOffice, and external model/embedding servers.

## 6. MCP Audit

### Industrial demo MCP

Configured in `.crudo/settings.local.json:2-9` as `industrial_demo`, using stdio and an absolute checkout-specific launcher path.

Tools:

- `extract_document`
- `ocr_document`
- `search_knowledge`
- `create_board_presentation`
- `generate_presentation`
- `verify_presentation`
- `create_approval_note`
- `verify_word_document`
- `run_code`
- `analyze_image`

Integration path:

```text
Crudo CLI
  -> RuntimeMcpState
  -> McpServerManager
  -> discover_tools_best_effort
  -> run_server.sh
  -> server.py
```

Rust MCP supports stdio lifecycle, JSON-RPC, tool discovery/calls, resources, retry/reset, and degraded reports. Non-stdio transports are modeled but unsupported by the manager.

Critical MCP issues:

- No maximum `Content-Length` before allocation in Rust and Python framing.
- Arbitrary configured stdio commands execute with host privileges.
- Demo launcher is not portable between checkouts.
- No pinned Python dependency manifest.
- No end-to-end JSON-RPC tests.
- `run_code` and `analyze_image` return unavailable/refused status without consistently using MCP error semantics.
- Environment-controlled Docling import path can supply arbitrary Python code.
- Path validation has resolve/use TOCTOU risk.

## 7. Actual Demo Workflow

The strongest current workflow is:

```text
User
  -> Crudo CLI
  -> industrial_demo stdio MCP
  -> local synthetic Markdown report
  -> optional Docling extraction
  -> optional PaddleOCR attempt
  -> keyword search over synthetic Markdown
  -> PPTX or DOCX generation
  -> text-marker checks
  -> LibreOffice temporary rendering
  -> verification result
```

The maximum complete honest workflow is a synthetic, human-reviewed document-to-artifact demo. It is not a scanned refinery report → OCR → MRPL RAG → engineering reasoning → verified approval workflow.

## 8. Test and Verification Audit

| Check | Result |
|---|---|
| Root Python tests | 47/47 passed |
| Demo tests without `PYTHONPATH` | Import failure for `office` |
| Demo tests with `PYTHONPATH=demo/mcp_server` | 1/2 failed |
| Rust workspace tests | 107 passed, 2 failed |
| `scripts/fmt.sh --check` | Failed |
| `cargo clippy --workspace --all-targets -- -D warnings` | Failed |
| Demo CI coverage | Absent |
| MCP E2E tests | Absent |
| Demo benchmarks | Absent |
| Offline clean-room test | Absent |
| MRPL E2E tests | Absent |

The Office test expects the missing fixture `demo/output/truck-count-board-deck.pptx` at `demo/tests/test_office_verification.py:13,21`. Existing outputs are different. LibreOffice reports `Error: source file could not be loaded`.

The demo tests are not included in GitHub workflows, and CI does not cover `demo/**`.

## 9. Security Threat Model

| Threat | Current protection | Severity | Required fix |
|---|---|---:|---|
| Document prompt injection | No strong trust boundary | High | Treat document/tool data as untrusted evidence |
| Malicious PDF/DOCX | Host-side parser/LibreOffice | High | Isolated non-root parser worker |
| Tool injection | Schemas only | High | Taint and validate tool outputs |
| Privilege escalation | Permission layer with bypass paths | High | Explicit deny/pending approval without prompter |
| Shell escape | Host shell fallback | Critical | Fail-closed container/VM |
| Sandbox escape | Best-effort unshare | Critical | Rootless container/microVM, seccomp, cgroups |
| Network exfiltration | No central deny | Critical | OS-level deny-all egress |
| DNS exfiltration | Not blocked centrally | High | No resolver/network namespace or controlled proxy |
| Credential leakage | Prompt/session/error/telemetry exposure | Critical | Secret redaction and safe auth types |
| Malicious MCP | Direct configured process execution | High | Trusted registry and isolated server |
| RAG poisoning | Weak trust metadata | High | Signed/versioned corpus and approval |
| Logs/telemetry leakage | Incomplete redaction | High | Redaction and retention controls |
| Symlink/path traversal | Resolve/check then use | Medium-high | Atomic no-follow file operations |
| MCP memory DoS | No frame-size limit | High | Reject oversized frames before allocation |
| Hook DoS | Shell hooks without strong timeout/isolation | High | Isolated timed hook runner |

### Secret-handling findings

- OAuth access/refresh tokens are persisted in plaintext under `~/.crudo/credentials.json` (`runtime/src/oauth.rs`).
- Provider API keys are written to settings (`runtime/src/config.rs:1111-1160`).
- Settings read/parse failures may be treated as empty configuration and later overwritten.
- Session redaction misses formats such as `apiKey`, `access_token`, `refresh_token`, lowercase bearer tokens, and arbitrary token formats.
- `Session::to_json` can expose unsanitized content blocks.
- Telemetry attributes are serialized without guaranteed redaction.
- Provider error bodies may retain raw response content.
- Auth/token structs derive debug formatting with raw token fields.
- HTTP proxy/client construction can fall back to a plain client if proxy configuration is malformed.

## 10. Provenance and Auditability

The current system partially records:

```text
user request
  -> session
  -> model request/response
  -> tool call
  -> tool result
  -> final response
```

It does not consistently record:

- selected model and routing reason,
- complete prompt/context lineage,
- document version/hash for every retrieval,
- page/region citations,
- OCR/VLM confidence,
- complete tool inputs/outputs,
- every network attempt/block,
- calculation inputs/formulas,
- artifact verification lineage,
- human reviewer identity and decision.

An MRPL-facing dashboard should show model/endpoints, locality, network policy and attempts, sandbox state, MCP servers, files accessed, documents/citations, routing reason, artifact hashes, verification, timestamps, and session ID.

## 11. What Not to Build

Deprioritize:

- unrelated educational/profile HTML pages,
- additional decorative presentation templates,
- generic web/search features in sovereign mode,
- more provider aliases,
- more generic agent personas and orchestration before industrial controls,
- dashboards that display claims rather than verified runtime facts.

The highest-value work is security, reproducibility, multimodal document handling, integrated retrieval, provenance, and one complete end-to-end workflow.

## 12. Top 10 Remaining Features

| Rank | Feature | MRPL value | Jury value | Difficulty | Priority |
|---:|---|---:|---:|---:|---:|
| 1 | Fail-closed sovereign execution profile | Very high | Very high | High | P0 |
| 2 | Integrated local document/OCR pipeline | Very high | Very high | High | P0 |
| 3 | Local VLM and P&ID/image workflow | Very high | Very high | High | P0 |
| 4 | Integrated RAG with citations | Very high | Very high | Medium-high | P0 |
| 5 | Deterministic task/modality model router | High | High | Medium | P1 |
| 6 | Provenance and audit event chain | Very high | Very high | Medium-high | P0 |
| 7 | Hardened DOCX/XLSX/PPTX/PDF artifact service | High | High | Medium-high | P1 |
| 8 | Realistic synthetic industrial corpus and E2E tests | Very high | Very high | Medium | P0 |
| 9 | Sovereignty dashboard | High | Very high | Medium | P1 |
| 10 | Pinned offline deployment bundle | Very high | High | Medium-high | P0 |

## 13. Implementation Strategies

### Strategy A — Minimum viable SIH

Build one local reasoning model, one local OCR/VLM path, one local embedding model, one document MCP, SQLite RAG, one approval-note workflow, artifact verification, network-disabled demo deployment, and basic provenance.

Expected result: credible local industrial workflow, but narrow capability and limited security depth.

### Strategy B — Strong finalist

Add deterministic routing, layout-aware extraction, page citations, OCR confidence, a P&ID image workflow, structured findings, artifact verification, reviewer gates, a sovereignty dashboard, pinned offline deployment, and adversarial security tests.

Expected result: substantially stronger industrial-product impression.

### Strategy C — Exceptional/winning-level

Add multimodal retrieval with region citations, handwriting support, engineering tag extraction, signed/versioned corpora, resource-limited calculation workers, cryptographic artifact lineage, role-separated approval, measured network-deny tests, attack simulation, reproducible offline deployment, and immutable audit events.

Expected result: serious industrial product profile, but substantially larger scope and validation burden.

## 14. Jury Questions the Current Repository Cannot Fully Answer

1. Does the complete system run without Internet? **CURRENTLY NOT SUPPORTED as a proven claim.**
2. Which exact local model is running? **Not bundled or verified by the repository.**
3. Can it process a scanned refinery PDF? **CURRENTLY NOT SUPPORTED reliably.**
4. Can it understand a P&ID? **CURRENTLY NOT SUPPORTED.**
5. Can it read handwritten notes? **CURRENTLY NOT SUPPORTED.**
6. Does the agent automatically retrieve MRPL knowledge? **CURRENTLY NOT SUPPORTED.**
7. Are citations page-specific? **CURRENTLY NOT SUPPORTED.**
8. Can it automatically choose a vision model? **CURRENTLY NOT SUPPORTED.**
9. Can it create validated XLSX calculations? **CURRENTLY NOT SUPPORTED.**
10. Is the approval note an actual operational approval? **No; it is a synthetic draft requiring human review.**
11. Is shell execution isolated? **Not reliably; fallback is host shell execution.**
12. Can it block DNS exfiltration? **CURRENTLY NOT SUPPORTED centrally.**
13. Can it prove no cloud calls occur? **CURRENTLY NOT SUPPORTED.**
14. Are Office verification tests green? **No; the current fixture is stale/missing.**
15. Are industrial MCP calls covered by E2E tests? **CURRENTLY NOT SUPPORTED.**
16. Are performance numbers available? **CURRENTLY NOT SUPPORTED.**
17. Can it show complete decision provenance? **Only partially.**
18. Are credentials protected from sessions/telemetry/errors? **Not reliably.**
19. Are all MCP transports supported? **No; operational support is stdio.**
20. Can it fail safely when local services are unavailable? **Only partially; several paths degrade or fail open.**

## 15. Final Verdict

### CURRENT SCORE

**5.0 / 10**

Estimated SIH26117 readiness: **35–40%**.

### WHAT WE HAVE

- Real streaming agent loop.
- Iterative model/tool execution.
- Provider abstraction and local-compatible endpoint support.
- Sessions, compaction, hooks, plugins, subagents, and task infrastructure.
- Local stdio MCP lifecycle.
- Industrial demo MCP server.
- Synthetic extraction, keyword search, PPTX, and DOCX workflow.
- Optional OCR integration path.
- LibreOffice rendering attempt.
- Basic artifact checks and some hashing.
- Standalone SQLite/vector RAG.
- Core Rust and root Python test surfaces.

### WHAT WE THOUGHT WE HAD BUT DON'T

- Complete sovereign deployment.
- Proven offline operation.
- Native multimodality.
- Local VLM workflow.
- P&ID/handwriting understanding.
- Integrated semantic MRPL RAG.
- Page/region citations.
- Automatic task-aware routing.
- Strong fail-closed sandboxing.
- Central network-deny enforcement.
- Complete provenance.
- XLSX generation/verification.
- Production-grade confidential-document security.
- Real MRPL E2E tests.
- Green repository-wide verification.

### CRITICAL BLOCKERS

1. Fail-open execution and network security.
2. No integrated multimodal industrial document pipeline.
3. RAG detached from normal agent reasoning.
4. No deterministic model router.
5. No real MRPL corpus or complete E2E evidence.

### NEXT 7 DAYS

**Day 1:** Fix Office fixture/import issues, remove unrelated demo noise, run all verification commands, and create an evidence matrix.

**Day 2:** Pin Python dependencies and build a reproducible local PDF/OCR/Docling environment with a scanned-PDF fixture.

**Day 3:** Add a typed retrieval MCP tool backed by the RAG service, including source hashes, chunk IDs, metadata, and citations.

**Day 4:** Add deterministic routing for document/image, retrieval, planning, coding, and artifact tasks; log the routing decision.

**Day 5:** Create a sovereign security profile: disable shell/web/remote tools, reject non-loopback endpoints, cap MCP frames, redact prompts/secrets, and fail closed when sandboxing is unavailable.

**Day 6:** Add a provenance/audit JSON record covering request, model, sources, citations, tool calls, artifact hash, verification, and human-review state.

**Day 7:** Rehearse one honest 10-minute local-only demo and capture successful logs and evidence.

### FINAL DEMO

```text
Local synthetic scanned inspection report
  -> document extraction and OCR
  -> confidence/page output
  -> integrated local RAG retrieval
  -> evidence-bound reasoning
  -> human approval checkpoint
  -> DOCX approval-note draft
  -> PPTX summary
  -> LibreOffice verification
  -> SHA-256 hashes
  -> audit report with citations and tool trace
```

The demo must explicitly show local endpoints, network denial, synthetic-data labeling, human review, visible failure of unsupported capabilities, and document prompt-injection handling.

### WINNING DIFFERENTIATOR

> **Evidence-bound, sovereignty-first industrial decisions: every recommendation and artifact is traceable to local document pages, OCR confidence, retrieved procedures, tool calls, model identity, verification results, and human approval—while the system fails closed whenever local security guarantees are unavailable.**

The current repository has the orchestration foundation for this differentiator, but not yet the security enforcement, multimodal pipeline, reproducibility, or provenance completeness needed to claim it.
