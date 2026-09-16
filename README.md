# Crudo

<p align="center">
  <a href="https://github.com/lu2gi2/crudo">lu2gi2/crudo</a>
  ·
  <a href="./USAGE.md">Usage</a>
  ·
  <a href="./rust/README.md">Rust workspace</a>
  ·
  <a href="./PARITY.md">Parity</a>
  ·
  <a href="./ROADMAP.md">Roadmap</a>
  ·
  <a href="./CONTRIBUTING.md">Contributing</a>
  ·
  <a href="./SECURITY.md">Security</a>
</p>

<p align="center">
  <a href="https://star-history.com/#lu2gi2/crudo&Date">
    <picture>
      <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=lu2gi2/crudo&type=Date&theme=dark" />
      <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=lu2gi2/crudo&type=Date" />
      <img alt="Star history for lu2gi2/crudo" src="https://api.star-history.com/svg?repos=lu2gi2/crudo&type=Date" width="600" />
    </picture>
  </a>
</p>

<p align="center">
  <img src="assets/crudo-hero.jpeg" alt="Crudo" width="300" />
</p>

Crudo is the public Rust implementation of the `crudo` CLI agent harness.
The canonical implementation lives in [`rust/`](./rust), and the current source of truth for this repository is **lu2gi2/crudo**.

> [!IMPORTANT]
> Start with [`USAGE.md`](./USAGE.md) for build, auth, CLI, session, and parity-harness workflows. For file submission/navigation questions, see [Navigation and file context](./docs/navigation-file-context.md). For local OpenAI-compatible models and offline skill installs, see [Local OpenAI-compatible providers and skills setup](./docs/local-openai-compatible-providers.md). Windows users can jump to the PowerShell-first [Windows install and release quickstart](./docs/windows-install-release.md). Make `crudo doctor` your first health check after building, use [`rust/README.md`](./rust/README.md) for crate-level details, read [`PARITY.md`](./PARITY.md) for the current Rust-port checkpoint, and see [`docs/container.md`](./docs/container.md) for the container-first workflow.
>
> **ACP / Zed status:** `crudo` does not ship an ACP/Zed daemon or JSON-RPC entrypoint yet. Run `crudo acp` (or `crudo --acp`) for the current status instead of guessing from source layout; `crudo acp serve` is currently a discoverability alias only, returns status with exit code 0, and real ACP support remains tracked separately in `ROADMAP.md`. For the public JSON contract, see [`docs/g011-acp-json-rpc-status-contract.md`](./docs/g011-acp-json-rpc-status-contract.md).

## Current repository shape

- **`rust/`** — canonical Rust workspace and the `crudo` CLI binary
- **`USAGE.md`** — task-oriented usage guide for the current product surface
- **`PARITY.md`** — Rust-port parity status and migration notes
- **`ROADMAP.md`** — dated, changelog-style log of past work and dogfooding findings, not a forward-looking backlog
- **`PHILOSOPHY.md`** — project intent and system-design framing
- **`src/` + `tests/`** — companion Python/reference workspace and audit helpers; not the primary runtime surface

## Quick start

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

### Windows setup

**PowerShell is a supported Windows path.** Use whichever shell works for you. The common onboarding issues on Windows are:

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

## Post-build: locate the binary and verify

After running `cargo build --workspace`, the `crudo` binary is built but **not** automatically installed to your system. Here's where to find it and how to verify the build succeeded.

### Binary location

After `cargo build --workspace` in `crudo/rust/`:

**Debug build (default, faster compile):**
- **macOS/Linux:** `rust/target/debug/crudo`
- **Windows:** `rust/target/debug/crudo.exe`

**Release build (optimized, slower compile):**
- **macOS/Linux:** `rust/target/release/crudo`
- **Windows:** `rust/target/release/crudo.exe`

If you ran `cargo build` without `--release`, the binary is in the `debug/` folder.

### Verify the build succeeded

Test the binary directly using its path:

```bash
# macOS/Linux (debug build)
./rust/target/debug/crudo --help
./rust/target/debug/crudo doctor

# Windows PowerShell (debug build)
.\rust\target\debug\crudo.exe --help
.\rust\target\debug\crudo.exe doctor
```

PowerShell smoke commands that do not require live credentials:

```powershell
$env:CRUDO_CONFIG_HOME = Join-Path $env:TEMP "crudo config home"
New-Item -ItemType Directory -Force -Path $env:CRUDO_CONFIG_HOME | Out-Null
Remove-Item Env:\ANTHROPIC_API_KEY, Env:\ANTHROPIC_AUTH_TOKEN, Env:\OPENAI_API_KEY -ErrorAction SilentlyContinue
.\rust\target\debug\crudo.exe help
.\rust\target\debug\crudo.exe status
.\rust\target\debug\crudo.exe config env
.\rust\target\debug\crudo.exe doctor
```

If these commands succeed, the build is working. `crudo doctor` is your first health check — it validates your API key, model access, and tool configuration.

### Optional: Add to PATH

If you want to run `crudo` from any directory without the full path, choose one of these approaches:

**Option 1: Symlink (macOS/Linux)**
```bash
ln -s $(pwd)/rust/target/debug/crudo /usr/local/bin/crudo
```
Then reload your shell and test:
```bash
crudo --help
```

**Option 2: Use `cargo install` (all platforms)**

Build and install to Cargo's default location (`~/.cargo/bin/`, which is usually on PATH):
```bash
# From the crudo/rust/ directory
cargo install --path . --force

# Then from anywhere
crudo --help
```

**Option 3: Update shell profile (bash/zsh)**

Add this line to `~/.bashrc` or `~/.zshrc`:
```bash
export PATH="$(pwd)/rust/target/debug:$PATH"
```

Reload your shell:
```bash
source ~/.bashrc  # or source ~/.zshrc
crudo --help
```

### Troubleshooting

- **"command not found: crudo"** — The binary is in `rust/target/debug/crudo`, but it's not on your PATH. Use the full path `./rust/target/debug/crudo` or symlink/install as above.
- **"permission denied"** — On macOS/Linux, you may need `chmod +x rust/target/debug/crudo` if the executable bit isn't set (rare).
- **Debug vs. release** — If the build is slow, you're in debug mode (default). Add `--release` to `cargo build` for faster runtime, but the build itself will take 5–10 minutes.

> [!NOTE]
> **Auth:** crudo requires an **API key** (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, etc.) — Claude subscription login is not a supported auth path.

Run the workspace test suite after verifying the binary works:

```bash
cd rust
cargo test --workspace
```

## Documentation map

- [`USAGE.md`](./USAGE.md) — quick commands, auth, sessions, config, parity harness
- [`docs/navigation-file-context.md`](./docs/navigation-file-context.md) — terminal navigation, scrollback, `@path` file context, attachments, and secret-safety guidance
- [`docs/local-openai-compatible-providers.md`](./docs/local-openai-compatible-providers.md) — Ollama/llama.cpp/vLLM setup, Crudo multi-provider positioning, and local skills install checks
- [`docs/windows-install-release.md`](./docs/windows-install-release.md) — PowerShell-first install, release artifact, provider switching, and Windows/WSL notification smoke paths
- [`rust/README.md`](./rust/README.md) — crate map, CLI surface, features, workspace layout
- [`PARITY.md`](./PARITY.md) — parity status for the Rust port
- [`rust/MOCK_PARITY_HARNESS.md`](./rust/MOCK_PARITY_HARNESS.md) — deterministic mock-service harness details
- [`ROADMAP.md`](./ROADMAP.md) — historical, dated log of implemented work and dogfooding findings
- [`docs/g004-events-reports-contract.md`](./docs/g004-events-reports-contract.md) — Stream 2 lane event/report contract guidance for consumers
- [`PHILOSOPHY.md`](./PHILOSOPHY.md) — why the project exists and how it is operated
- [`CONTRIBUTING.md`](./CONTRIBUTING.md), [`SECURITY.md`](./SECURITY.md), [`SUPPORT.md`](./SUPPORT.md), and [`CODE_OF_CONDUCT.md`](./CODE_OF_CONDUCT.md) — contribution, vulnerability-reporting, support, and community policies
- [`LICENSE`](./LICENSE) — MIT license for this repository

## Ownership / affiliation disclaimer

- This repository does **not** claim ownership of the original Claude Code source material.
- This repository is **not affiliated with, endorsed by, or maintained by Anthropic**.
