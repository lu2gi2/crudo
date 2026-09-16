<p align="center">
  <img src="assets/crudo-hero.png" alt="Crudo" width="450" />
</p>

<p align="center"><i>a Rust-built coding-agent harness, run off the factory floor</i></p>

<p align="center">
  <a href="https://github.com/lu2gi2/crudo">lu2gi2/crudo</a>
  ·
  <a href="./USAGE.md">Usage</a>
  ·
  <a href="./rust/README.md">Rust workspace</a>
  ·
  <a href="./PARITY.md">Parity</a>
  ·
  <a href="./ROADMAP.md">Build log</a>
  ·
  <a href="./CONTRIBUTING.md">Contributing</a>
  ·
  <a href="./SECURITY.md">Security</a>
</p>

---

## What's running on the floor

Crudo is a Rust CLI that drives an LLM through a tool loop — read and write files, run shell commands, search a codebase, call MCP servers — gated by a permission system that decides what the agent is actually allowed to touch. [`rust/`](./rust) is the factory floor itself: the canonical Cargo workspace and the `crudo` binary. Everything else in this repository — docs, the build log, the Python reference harness — is scaffolding around it.

> [!IMPORTANT]
> Start with [`USAGE.md`](./USAGE.md) for build, auth, CLI, session, and parity-harness workflows. Make `crudo doctor` your first health check after building. For crate-level detail see [`rust/README.md`](./rust/README.md); for the current Rust-port checkpoint see [`PARITY.md`](./PARITY.md); for the container-first workflow see [`docs/container.md`](./docs/container.md).
>
> **ACP / Zed status:** `crudo` does not ship an ACP/Zed daemon or JSON-RPC entrypoint yet. Run `crudo acp` (or `crudo --acp`) for the current status instead of guessing from source layout; `crudo acp serve` is currently a discoverability alias only, returns status with exit code 0, and real ACP support remains tracked separately in `ROADMAP.md`. For the public JSON contract, see [`docs/g011-acp-json-rpc-status-contract.md`](./docs/g011-acp-json-rpc-status-contract.md).

## Assembly line: build & run

> [!WARNING]
> **`cargo install crudo` installs the wrong thing.** The `crudo` crate on crates.io is a deprecated stub that places `crudo-deprecated.exe` — not `crudo`. Running it only prints `"crudo has been renamed to agent-code"`. **Do not use `cargo install crudo`.** Either build from source (this repo) or install the upstream binary:
> ```bash
> cargo install agent-code   # upstream binary — installs 'agent.exe' (Windows) / 'agent' (Unix), NOT 'agent-code'
> ```
> This repo (`lu2gi2/crudo`) is **build-from-source only** — follow the steps below.

```bash
# 1. Clone and build
git clone https://github.com/lu2gi2/crudo
cd crudo/rust
cargo build --workspace

# 2. Set your API key (Anthropic API key — not a Claude subscription)
export ANTHROPIC_API_KEY="sk-ant-..."

# 3. Verify everything is wired correctly
./target/debug/crudo doctor

# 4. Run a prompt
./target/debug/crudo prompt "say hello"

# 5. Start an interactive session
./target/debug/crudo
```

> [!NOTE]
> **Windows (PowerShell):** the binary is `crudo.exe`, not `crudo`. Use `.\target\debug\crudo.exe` or run `cargo run -- prompt "say hello"` to skip the path lookup.

<details>
<summary><b>Windows / PowerShell line — full setup</b></summary>

**PowerShell is a supported Windows path.** Use whichever shell works for you. The common onboarding steps:

1. **Install Rust first** — download from <https://rustup.rs/> and run the installer. Close and reopen your terminal when it finishes.
2. **Configure and install Crudo** — run `bash scripts/install-crudo.sh` and choose cloud endpoint, direct provider key, or local/self-hosted model. Direct mode includes named Gemini, Groq, and OpenRouter presets as well as Anthropic, OpenAI, xAI, DashScope, and custom OpenAI-compatible endpoints. This installs the normal `crudo` command; `scripts/install-crudo-luna.sh` is retained only for compatibility.
3. **Verify Rust is on PATH:**
   ```powershell
   cargo --version
   ```
   If this fails, reopen your terminal or run the PATH setup from the Rust installer output, then retry.
4. **Clone and build** (works in PowerShell, Git Bash, or WSL):
   ```powershell
   git clone https://github.com/lu2gi2/crudo
   cd crudo/rust
   cargo build --workspace
   ```
5. **Run** (PowerShell — note `.exe` and backslash):
   ```powershell
   $env:ANTHROPIC_API_KEY = "sk-ant-..."
   .\target\debug\crudo.exe prompt "say hello"
   ```

For release ZIPs, PATH setup, provider switching, and notification smoke checks, see [`docs/windows-install-release.md`](./docs/windows-install-release.md).

**Git Bash / WSL** are optional alternatives, not requirements. If you prefer bash-style paths (`/c/Users/you/...` instead of `C:\Users\you\...`), Git Bash (ships with Git for Windows) works well. In Git Bash, the `MINGW64` prompt is expected and normal — not a broken install.

</details>

## Quality control: verify the build

After `cargo build --workspace`, the `crudo` binary exists but is **not** automatically installed to your system.

**Where it lands:**

| Build   | macOS/Linux                | Windows                        |
|---------|-----------------------------|---------------------------------|
| Debug (default) | `rust/target/debug/crudo` | `rust/target/debug/crudo.exe` |
| Release (`--release`) | `rust/target/release/crudo` | `rust/target/release/crudo.exe` |

**Smoke-test it:**

```bash
# macOS/Linux (debug build)
./rust/target/debug/crudo --help
./rust/target/debug/crudo doctor
```

```powershell
# Windows PowerShell — no live credentials required
$env:CRUDO_CONFIG_HOME = Join-Path $env:TEMP "crudo config home"
New-Item -ItemType Directory -Force -Path $env:CRUDO_CONFIG_HOME | Out-Null
Remove-Item Env:\ANTHROPIC_API_KEY, Env:\ANTHROPIC_AUTH_TOKEN, Env:\OPENAI_API_KEY -ErrorAction SilentlyContinue
.\rust\target\debug\crudo.exe help
.\rust\target\debug\crudo.exe status
.\rust\target\debug\crudo.exe config env
.\rust\target\debug\crudo.exe doctor
```

If these succeed, the build is working. `crudo doctor` is your first health check — it validates your API key, model access, and tool configuration.

Then run the full workspace test suite:

```bash
cd rust
cargo test --workspace
```

> [!NOTE]
> **Auth:** crudo requires an **API key** (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, etc.) — Claude subscription login is not a supported auth path.

<details>
<summary>Put <code>crudo</code> on your PATH, and troubleshoot a build that doesn't run</summary>

**Option 1: Symlink (macOS/Linux)**
```bash
ln -s $(pwd)/rust/target/debug/crudo /usr/local/bin/crudo
crudo --help
```

**Option 2: `cargo install` (all platforms)**
```bash
# From the crudo/rust/ directory
cargo install --path . --force
crudo --help
```

**Option 3: Shell profile (bash/zsh)**
```bash
echo 'export PATH="'"$(pwd)"'/rust/target/debug:$PATH"' >> ~/.bashrc  # or ~/.zshrc
source ~/.bashrc
crudo --help
```

**Common failures:**
- **"command not found: crudo"** — the binary is in `rust/target/debug/crudo`, but it's not on your PATH. Use the full path or one of the options above.
- **"permission denied"** — on macOS/Linux, run `chmod +x rust/target/debug/crudo` if the executable bit isn't set (rare).
- **Debug vs. release** — debug mode is the default and compiles faster; add `--release` for faster *runtime*, at the cost of a 5–10 minute build.

</details>

## Floor plan

```text
crudo/
├── rust/            canonical Cargo workspace — 11 crates, the `crudo` CLI binary
├── docs/            reference docs: providers, sessions, contracts, release gates
├── USAGE.md         task-oriented usage guide for the current product surface
├── PARITY.md        Rust-port parity status and migration notes
├── ROADMAP.md       dated, changelog-style log of past work — history, not a live backlog
├── PHILOSOPHY.md     project intent and system-design framing
└── src/ + tests/    companion Python reference workspace and audit helpers (non-primary)
```

## Manuals

**Getting running**
- [`USAGE.md`](./USAGE.md) — quick commands, auth, sessions, config, parity harness
- [`docs/windows-install-release.md`](./docs/windows-install-release.md) — PowerShell-first install, release artifact, provider switching, Windows/WSL notification smoke paths
- [`docs/container.md`](./docs/container.md) — container-first workflow
- [`docs/navigation-file-context.md`](./docs/navigation-file-context.md) — terminal navigation, scrollback, `@path` file context, attachments, secret-safety guidance

**Providers & models**
- [`docs/local-openai-compatible-providers.md`](./docs/local-openai-compatible-providers.md) — Ollama/llama.cpp/vLLM setup, multi-provider positioning, local skills install checks
- [`docs/MODEL_COMPATIBILITY.md`](./docs/MODEL_COMPATIBILITY.md) — tested models, aliases, and compatibility notes

**Internals & contracts**
- [`rust/README.md`](./rust/README.md) — crate map, CLI surface, features, workspace layout
- [`PARITY.md`](./PARITY.md) — parity status for the Rust port
- [`rust/MOCK_PARITY_HARNESS.md`](./rust/MOCK_PARITY_HARNESS.md) — deterministic mock-service harness details
- [`docs/g004-events-reports-contract.md`](./docs/g004-events-reports-contract.md) — lane event/report contract for consumers
- [`docs/g011-acp-json-rpc-status-contract.md`](./docs/g011-acp-json-rpc-status-contract.md) — ACP/JSON-RPC status contract

**Project & policy**
- [`PHILOSOPHY.md`](./PHILOSOPHY.md) — why the project exists and how it is operated
- [`ROADMAP.md`](./ROADMAP.md) — historical, dated log of implemented work and dogfooding findings
- [`CONTRIBUTING.md`](./CONTRIBUTING.md), [`SECURITY.md`](./SECURITY.md), [`SUPPORT.md`](./SUPPORT.md), [`CODE_OF_CONDUCT.md`](./CODE_OF_CONDUCT.md) — contribution, vulnerability-reporting, support, and community policies
- [`LICENSE`](./LICENSE) — MIT license for this repository

## Why this exists

The short version, from [`PHILOSOPHY.md`](./PHILOSOPHY.md): a human sets direction, and the agents that live in this repo — the crudos — break it into tasks, write the code, run the tests, argue over failures, recover, and push when the work passes. The repository is the artifact; the coordination loop that produced it is the actual point.

## Ownership / affiliation disclaimer

- This repository does **not** claim ownership of the original Claude Code source material.
- This repository is **not affiliated with, endorsed by, or maintained by Anthropic**.
