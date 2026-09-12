#!/usr/bin/env bash
# Configure and install the provider-neutral Crudo command.
#
# Modes:
#   cloud  - remote Anthropic Messages-compatible endpoint
#   direct - Anthropic, OpenAI, Gemini, Groq, OpenRouter, xAI, DashScope, or custom OpenAI-compatible
#   local  - Ollama or another local OpenAI-compatible server
#
# Secrets are stored only in ~/.config/nuvai/crudo/config.env (0600).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
INSTALL_DIR="${HOME}/.local/bin"
CONFIG_DIR="${HOME}/.config/nuvai/crudo"
CONFIG_FILE="$CONFIG_DIR/config.env"
TARGET="$INSTALL_DIR/crudo"
BUILD_PROFILE="${CRUDO_BUILD_PROFILE:-release}"
SKIP_VERIFY="${CRUDO_SKIP_VERIFY:-0}"
MODE="${CRUDO_SETUP_MODE:-}"
PROVIDER="${CRUDO_SETUP_PROVIDER:-}"
ENDPOINT="${CRUDO_SETUP_ENDPOINT:-}"
API_KEY="${CRUDO_SETUP_API_KEY:-}"
MODEL="${CRUDO_SETUP_MODEL:-}"
THINKING="${CRUDO_SETUP_THINKING:-auto}"
LOCAL_KIND="${CRUDO_SETUP_LOCAL_KIND:-generic}"
FORCE="${CRUDO_SETUP_FORCE:-0}"
NON_INTERACTIVE="${CRUDO_SETUP_NONINTERACTIVE:-0}"

usage() {
  cat <<'EOF'
Usage: scripts/install-crudo.sh [options]

Options:
  --mode cloud|direct|local       Select setup mode (otherwise prompt).
  --provider NAME                 direct provider: anthropic|openai|gemini|groq|openrouter|xai|dashscope|custom.
  --endpoint URL                  Cloud/custom/local endpoint.
  --api-key KEY                   API key (prefer CRUDO_SETUP_API_KEY in CI).
  --model MODEL                   Exact model/deployment identifier.
  --thinking auto|on|off          Thinking mode (default: auto).
  --local-kind ollama|generic     Local server protocol (default: generic).
  --release | --debug             Build profile (default: release).
  --no-verify                     Skip --version/--help verification.
  --force                         Replace an existing non-Crudo ~/.local/bin/crudo.
  --non-interactive               Reject missing values instead of prompting.
  -h, --help                      Show this help.

Environment equivalents:
  CRUDO_SETUP_MODE, CRUDO_SETUP_PROVIDER, CRUDO_SETUP_ENDPOINT,
  CRUDO_SETUP_API_KEY, CRUDO_SETUP_MODEL, CRUDO_SETUP_THINKING, CRUDO_SETUP_LOCAL_KIND,
  CRUDO_BUILD_PROFILE, CRUDO_SKIP_VERIFY, CRUDO_SETUP_FORCE,
  CRUDO_SETUP_NONINTERACTIVE

The cloud mode expects an Anthropic Messages-compatible API root, like the
endpoint configured by install-crudo-luna.sh. Local mode supports Ollama and
OpenAI-compatible /v1 servers without a cloud API key.
EOF
}

fail() { printf 'Error: %s\n' "$1" >&2; exit 1; }
info() { printf '%s\n' "$1"; }

while (($#)); do
  case "$1" in
    --mode) (($# >= 2)) || fail "--mode requires a value"; MODE="$2"; shift 2 ;;
    --provider) (($# >= 2)) || fail "--provider requires a value"; PROVIDER="$2"; shift 2 ;;
    --endpoint) (($# >= 2)) || fail "--endpoint requires a value"; ENDPOINT="$2"; shift 2 ;;
    --api-key) (($# >= 2)) || fail "--api-key requires a value"; API_KEY="$2"; shift 2 ;;
    --model) (($# >= 2)) || fail "--model requires a value"; MODEL="$2"; shift 2 ;;
    --thinking) (($# >= 2)) || fail "--thinking requires a value"; THINKING="$2"; shift 2 ;;
    --local-kind) (($# >= 2)) || fail "--local-kind requires a value"; LOCAL_KIND="$2"; shift 2 ;;
    --release) BUILD_PROFILE=release; shift ;;
    --debug) BUILD_PROFILE=debug; shift ;;
    --no-verify) SKIP_VERIFY=1; shift ;;
    --force) FORCE=1; shift ;;
    --non-interactive) NON_INTERACTIVE=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) fail "unknown option: $1" ;;
  esac
done

[[ "$BUILD_PROFILE" == debug || "$BUILD_PROFILE" == release ]] || fail "build profile must be debug or release"
[[ "$THINKING" == auto || "$THINKING" == on || "$THINKING" == off ]] || fail "thinking mode must be auto, on, or off"
[[ -d "$REPO_ROOT/rust" && -f "$REPO_ROOT/rust/Cargo.toml" ]] || fail "Rust workspace not found next to installer"

prompt() {
  local name="$1" label="$2" secret="${3:-0}" value=""
  if [[ "$NON_INTERACTIVE" == 1 ]]; then
    return 0
  fi
  if [[ "$secret" == 1 ]]; then
    read -r -s -p "$label" value
    printf '\n' >&2
  else
    read -r -p "$label" value
  fi
  printf -v "$name" '%s' "$value"
}

if [[ -z "$MODE" ]]; then
  [[ "$NON_INTERACTIVE" == 1 ]] && fail "--mode is required with --non-interactive"
  cat <<'EOF'

Crudo setup
  1) Cloud endpoint (remote Anthropic-compatible API)
  2) Direct provider API key (Anthropic/OpenAI/Gemini/Groq/OpenRouter/xAI/DashScope/custom)
  3) Local or self-hosted model (Ollama/OpenAI-compatible)
EOF
  read -r -p 'Choose [1-3]: ' choice
  case "$choice" in 1) MODE=cloud ;; 2) MODE=direct ;; 3) MODE=local ;; *) fail "choose 1, 2, or 3" ;; esac
fi

case "$MODE" in
  cloud)
    ENDPOINT="${ENDPOINT:-${CRUDO_LUNA_ENDPOINT:-http://20.230.232.136:8081}}"
    [[ -n "$ENDPOINT" ]] || prompt ENDPOINT 'Cloud API root: '
    [[ -n "$API_KEY" ]] || API_KEY="${CRUDO_LUNA_API_KEY:-${LUNACODE_API_KEY:-}}"
    [[ -n "$API_KEY" ]] || prompt API_KEY 'Cloud API key: ' 1
    [[ -n "$API_KEY" ]] || fail "an API key is required for cloud mode"
    MODEL="${MODEL:-${CRUDO_LUNA_MODEL:-${LUNACODE_MODEL:-gpt-5.6-luna}}}"
    [[ -n "$MODEL" ]] || prompt MODEL 'Cloud model: '
    [[ -n "$MODEL" ]] || fail "a model is required for cloud mode"
    PROVIDER=anthropic-compatible
    ;;
  direct)
    if [[ -z "$PROVIDER" ]]; then
      [[ "$NON_INTERACTIVE" == 1 ]] && fail "--provider is required for direct mode"
      cat <<'EOF'

Direct provider:
  1) Anthropic
  2) OpenAI
  3) Gemini (Google)
  4) Groq
  5) OpenRouter
  6) xAI
  7) DashScope (Qwen/Kimi)
  8) Custom OpenAI-compatible
EOF
      read -r -p 'Choose [1-8]: ' choice
      case "$choice" in
        1) PROVIDER=anthropic ;;
        2) PROVIDER=openai ;;
        3) PROVIDER=gemini ;;
        4) PROVIDER=groq ;;
        5) PROVIDER=openrouter ;;
        6) PROVIDER=xai ;;
        7) PROVIDER=dashscope ;;
        8) PROVIDER=custom ;;
        *) fail "choose 1-8" ;;
      esac
    fi
    case "$PROVIDER" in anthropic|openai|gemini|groq|openrouter|xai|dashscope|custom) ;; *) fail "unsupported direct provider: $PROVIDER" ;; esac
    case "$PROVIDER" in
      gemini) API_KEY="${API_KEY:-${GEMINI_API_KEY:-}}"; ENDPOINT="${ENDPOINT:-https://generativelanguage.googleapis.com/v1beta/openai}" ;;
      groq) API_KEY="${API_KEY:-${GROQ_API_KEY:-}}"; ENDPOINT="${ENDPOINT:-https://api.groq.com/openai/v1}" ;;
      openrouter) API_KEY="${API_KEY:-${OPENROUTER_API_KEY:-}}"; ENDPOINT="${ENDPOINT:-https://openrouter.ai/api/v1}" ;;
    esac
    [[ -n "$API_KEY" ]] || API_KEY="${ANTHROPIC_API_KEY:-${OPENAI_API_KEY:-${XAI_API_KEY:-${DASHSCOPE_API_KEY:-}}}}"
    [[ -n "$API_KEY" ]] || prompt API_KEY 'Provider API key: ' 1
    [[ -n "$API_KEY" ]] || fail "an API key is required for direct mode"
    if [[ "$PROVIDER" == custom || "$PROVIDER" == openai || "$PROVIDER" == gemini || "$PROVIDER" == groq || "$PROVIDER" == openrouter || "$PROVIDER" == xai || "$PROVIDER" == dashscope ]]; then
      [[ -n "$ENDPOINT" ]] || prompt ENDPOINT 'API base URL (blank for provider default): '
    fi
    [[ -n "$MODEL" ]] || prompt MODEL 'Model: '
    [[ -n "$MODEL" ]] || fail "a model is required for direct mode"
    ;;
  local)
    [[ "$LOCAL_KIND" == ollama || "$LOCAL_KIND" == generic ]] || fail "local kind must be ollama or generic"
    [[ -n "$ENDPOINT" ]] || prompt ENDPOINT "Local ${LOCAL_KIND} URL: "
    if [[ -z "$ENDPOINT" ]]; then
      [[ "$LOCAL_KIND" == ollama ]] && ENDPOINT=http://127.0.0.1:11434 || ENDPOINT=http://127.0.0.1:8000/v1
    fi
    [[ -n "$MODEL" ]] || prompt MODEL 'Exact local model ID: '
    [[ -n "$MODEL" ]] || fail "a model is required for local mode"
    PROVIDER="$LOCAL_KIND"
    # A local server may be authless. Use a harmless placeholder if it emits
    # an Authorization header requirement; never reuse an ambient cloud key.
    API_KEY="${API_KEY:-local-dev-token}"
    ;;
  *) fail "mode must be cloud, direct, or local" ;;
esac

normalize_endpoint() {
  ENDPOINT="${ENDPOINT%/}"
  [[ -z "$ENDPOINT" && "$MODE" == direct ]] && return
  [[ "$ENDPOINT" =~ ^https?:// ]] || fail "endpoint must start with http:// or https://"
  [[ "$ENDPOINT" != */health && "$ENDPOINT" != */v1/messages ]] || fail "endpoint must be an API root, not /health or /v1/messages"
}
normalize_endpoint

case "$MODE:$LOCAL_KIND" in
  local:ollama) ENDPOINT="${ENDPOINT%/}"; ENDPOINT="${ENDPOINT%/v1}" ;;
  local:generic) [[ "$ENDPOINT" == */v1 ]] || ENDPOINT="${ENDPOINT}/v1" ;;
esac

if [[ "$MODE" == cloud || "$MODE" == direct && "$PROVIDER" == custom ]]; then
  if [[ "$ENDPOINT" != http://127.0.0.1:* && "$ENDPOINT" != http://localhost:* && "$ENDPOINT" != https://127.0.0.1:* && "$ENDPOINT" != https://localhost:* ]]; then
    printf 'WARNING: this configuration sends prompts and files to a remote endpoint.\n' >&2
  fi
fi

mkdir -p "$INSTALL_DIR"
(umask 077; mkdir -p "$CONFIG_DIR")
chmod 700 "$CONFIG_DIR"

# Refuse to replace an unrelated command. A prior generated launcher contains
# the marker below, making reconfiguration idempotent.
if [[ -e "$TARGET" && "$(grep -Fcs 'Crudo provider-neutral launcher' "$TARGET" 2>/dev/null || true)" == 0 && "$FORCE" != 1 ]]; then
  fail "$TARGET already exists; use --force to replace it"
fi

(umask 077; {
  printf '# Generated by scripts/install-crudo.sh\n'
  printf 'CRUDO_SETUP_MODE=%q\n' "$MODE"
  printf 'CRUDO_SETUP_PROVIDER=%q\n' "$PROVIDER"
  printf 'CRUDO_SETUP_ENDPOINT=%q\n' "$ENDPOINT"
  printf 'CRUDO_SETUP_API_KEY=%q\n' "$API_KEY"
  printf 'CRUDO_SETUP_MODEL=%q\n' "$MODEL"
  printf 'CRUDO_SETUP_THINKING=%q\n' "$THINKING"
  printf 'CRUDO_SETUP_LOCAL_KIND=%q\n' "$LOCAL_KIND"
  printf 'CRUDO_REPO_ROOT=%q\n' "$REPO_ROOT"
  printf 'CRUDO_BUILD_PROFILE=%q\n' "$BUILD_PROFILE"
} > "$CONFIG_FILE")
chmod 600 "$CONFIG_FILE"

case "$BUILD_PROFILE" in
  release) (cd "$REPO_ROOT/rust" && cargo build --workspace --release) ;;
  debug) (cd "$REPO_ROOT/rust" && cargo build --workspace) ;;
esac
CRUDO_BIN="$REPO_ROOT/rust/target/$BUILD_PROFILE/crudo"
[[ -x "$CRUDO_BIN" ]] || fail "built crudo binary not found at $CRUDO_BIN"

cat > "$TARGET" <<'LAUNCHER'
#!/usr/bin/env bash
# Crudo provider-neutral launcher
# Crudo provider-neutral launcher
set -euo pipefail
CONFIG_FILE="${HOME}/.config/nuvai/crudo/config.env"
[[ -r "$CONFIG_FILE" ]] || { printf 'Crudo is not configured. Run scripts/install-crudo.sh.\n' >&2; exit 1; }
# shellcheck disable=SC1090
source "$CONFIG_FILE"
: "${CRUDO_SETUP_MODE:?missing setup mode}"
: "${CRUDO_SETUP_MODEL:?missing model}"
: "${CRUDO_REPO_ROOT:?missing repository root}"

unset ANTHROPIC_API_KEY ANTHROPIC_AUTH_TOKEN ANTHROPIC_BASE_URL
unset OPENAI_API_KEY OPENAI_BASE_URL XAI_API_KEY XAI_BASE_URL DASHSCOPE_API_KEY DASHSCOPE_BASE_URL OLLAMA_HOST
case "$CRUDO_SETUP_MODE:$CRUDO_SETUP_PROVIDER:$CRUDO_SETUP_LOCAL_KIND" in
  cloud:*)
    export ANTHROPIC_BASE_URL="$CRUDO_SETUP_ENDPOINT" ANTHROPIC_API_KEY="$CRUDO_SETUP_API_KEY"
    export ANTHROPIC_AUTH_TOKEN="$CRUDO_SETUP_API_KEY" CRUDO_MODEL="anthropic/${CRUDO_SETUP_MODEL#anthropic/}"
    ;;
  direct:anthropic:*)
    export ANTHROPIC_API_KEY="$CRUDO_SETUP_API_KEY" CRUDO_MODEL="${CRUDO_SETUP_MODEL#anthropic/}"
    [[ -n "${CRUDO_SETUP_ENDPOINT:-}" ]] && export ANTHROPIC_BASE_URL="$CRUDO_SETUP_ENDPOINT"
    ;;
  direct:openai:*|direct:custom:*)
    export OPENAI_API_KEY="$CRUDO_SETUP_API_KEY" CRUDO_MODEL="openai/${CRUDO_SETUP_MODEL#openai/}"
    [[ -n "${CRUDO_SETUP_ENDPOINT:-}" ]] && export OPENAI_BASE_URL="$CRUDO_SETUP_ENDPOINT"
    ;;
  direct:gemini:*|direct:groq:*)
    # These named providers accept bare model IDs. The launcher clears other
    # credentials, so the OpenAI-compatible detector remains deterministic.
    export OPENAI_API_KEY="$CRUDO_SETUP_API_KEY" CRUDO_MODEL="${CRUDO_SETUP_MODEL#openai/}"
    [[ -n "${CRUDO_SETUP_ENDPOINT:-}" ]] && export OPENAI_BASE_URL="$CRUDO_SETUP_ENDPOINT"
    ;;
  direct:openrouter:*)
    # OpenRouter model IDs commonly contain a provider slash; preserve it.
    export OPENAI_API_KEY="$CRUDO_SETUP_API_KEY" CRUDO_MODEL="openai/${CRUDO_SETUP_MODEL#openai/}"
    [[ -n "${CRUDO_SETUP_ENDPOINT:-}" ]] && export OPENAI_BASE_URL="$CRUDO_SETUP_ENDPOINT"
    ;;
  direct:xai:*)
    export XAI_API_KEY="$CRUDO_SETUP_API_KEY" CRUDO_MODEL="${CRUDO_SETUP_MODEL#grok/}"
    [[ -n "${CRUDO_SETUP_ENDPOINT:-}" ]] && export XAI_BASE_URL="$CRUDO_SETUP_ENDPOINT"
    ;;
  direct:dashscope:*)
    export DASHSCOPE_API_KEY="$CRUDO_SETUP_API_KEY" CRUDO_MODEL="${CRUDO_SETUP_MODEL#dashscope/}"
    [[ -n "${CRUDO_SETUP_ENDPOINT:-}" ]] && export DASHSCOPE_BASE_URL="$CRUDO_SETUP_ENDPOINT"
    ;;
  local:ollama:*)
    export OLLAMA_HOST="${CRUDO_SETUP_ENDPOINT%/}" CRUDO_MODEL="$CRUDO_SETUP_MODEL"
    ;;
  local:generic:*)
    export OPENAI_BASE_URL="$CRUDO_SETUP_ENDPOINT" OPENAI_API_KEY="${CRUDO_SETUP_API_KEY:-local-dev-token}"
    export CRUDO_MODEL="local/${CRUDO_SETUP_MODEL#local/}"
    ;;
  *) printf 'Invalid Crudo configuration. Re-run scripts/install-crudo.sh.\n' >&2; exit 1 ;;
esac
export ANTHROPIC_MODEL="$CRUDO_MODEL" CLAUDE_CODE_SUBAGENT_MODEL="$CRUDO_MODEL"
export CRUDO_THINKING_MODE="${CRUDO_SETUP_THINKING:-auto}"

if [[ "${1:-}" == --config-status ]]; then
  printf 'mode=%s\nprovider=%s\nendpoint=%s\nmodel=%s\nthinking=%s\napi_key=loaded\nconfig=%s\n' \
    "$CRUDO_SETUP_MODE" "$CRUDO_SETUP_PROVIDER" "${CRUDO_SETUP_ENDPOINT:-default}" "$CRUDO_SETUP_MODEL" "$CRUDO_THINKING_MODE" "$CONFIG_FILE"
  exit 0
fi
cd "$CRUDO_REPO_ROOT"
exec "$CRUDO_REPO_ROOT/rust/target/$CRUDO_BUILD_PROFILE/crudo" "$@"
LAUNCHER
chmod 755 "$TARGET"

case ":${PATH}:" in *":$INSTALL_DIR:"*) ;; *)
  shell_rc="$HOME/.bashrc"; [[ "${SHELL##*/}" == zsh ]] && shell_rc="$HOME/.zshrc"
  path_line='export PATH="$HOME/.local/bin:$PATH"'
  if [[ ! -f "$shell_rc" ]] || ! grep -Fqx "$path_line" "$shell_rc"; then printf '\n# Crudo command\n%s\n' "$path_line" >> "$shell_rc"; fi
  printf 'Added ~/.local/bin to %s; open a new shell or source it.\n' "$shell_rc"
  ;; esac

if [[ "$SKIP_VERIFY" != 1 ]]; then
  "$TARGET" --version >/dev/null
  "$TARGET" --help >/dev/null
fi
printf 'Installed %s\nConfigured %s mode for model %s\nRun: crudo --config-status\n' "$TARGET" "$MODE" "$MODEL"
