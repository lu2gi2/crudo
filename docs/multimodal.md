# Multimodal Input and Local Vision

## Overview

Crudo's multimodal work adds a controlled input path for local images and other document attachments. The design is provider-neutral: the chat/runtime can carry an image without assuming that a particular vision-language model is installed. When a compatible local model is configured, the same message can be serialized for that model.

The current tested target is an Ollama-hosted Qwen3-VL model. The implementation is deliberately incremental:

```text
user prompt + local image
        ↓
attachment validation
        ↓
conversation image block
        ↓
provider-neutral API message
        ↓
OpenAI-compatible image_url data URL
        ↓
Ollama / Qwen3-VL
```

Crudo does not claim that a VLM is bundled. A local model server and model assets are still required for actual visual inference.

## Supported attachment contract

The API defines these attachment categories:

- `image`
- `pdf`
- `document`
- `spreadsheet`
- `text`

The current provider wire is focused on images. The other categories are represented and validated so document-specific processing can be added without changing the basic input contract.

Supported extensions currently include:

```text
png, jpg, jpeg, webp
pdf
 doc, docx
txt, md
csv, xls, xlsx
```

The central type is `api::MultimodalAttachment` in `rust/crates/api/src/types.rs`. It records:

- canonical local path
- filename
- detected media type
- attachment kind
- byte size
- BLAKE3 content hash
- `local_only` marker

## Validation and security

Attachments are validated before they become model input. `MultimodalAttachment::from_path` requires:

1. The path can be canonicalized.
2. The path is inside the supplied workspace root.
3. The target is a regular file.
4. The file is no larger than 20 MiB.
5. The extension is supported.
6. The file can be read locally.

An attachment outside the workspace is rejected with `OutsideWorkspace`. Unsupported files are rejected with `UnsupportedType`, and oversized files produce `TooLarge`.

The image bytes are never fetched from a URL by this path. `InputMessage::user_image` reads the validated local file and encodes it in memory as base64. The resulting API block is marked conceptually local by the originating attachment metadata; the provider receives a data URL rather than an external URL.

This boundary is separate from general filesystem tool permissions. Allowing an explicit attachment does not grant the model broad access to the source directory.

## Provider-neutral API representation

`api::InputContentBlock` supports an image block:

```rust
InputContentBlock::Image {
    media_type: String,
    data: String,
}
```

`InputMessage::user_image` creates a user message containing this block. The message remains compatible with the existing text/tool message model.

For OpenAI-compatible providers, including Ollama's compatibility endpoint, the image is translated to:

```json
{
  "role": "user",
  "content": [
    {
      "type": "image_url",
      "image_url": {
        "url": "data:image/png;base64,..."
      }
    }
  ]
}
```

The conversion is implemented in `rust/crates/api/src/providers/openai_compat.rs`. This is a wire-format adapter; it does not assert that every OpenAI-compatible provider supports vision.

## Runtime and session support

The runtime session model now supports:

```rust
ContentBlock::Image {
    media_type: String,
    data: String,
}
```

`ConversationRuntime::run_turn_with_message` accepts a complete `ConversationMessage`, while the existing text-only `run_turn` API remains available.

Image blocks are handled by:

- session JSONL persistence and restoration
- provider message conversion
- compaction summaries
- token estimation
- runtime fingerprints
- text and Markdown exports
- tool-side message conversion

Exports intentionally show image metadata instead of dumping base64 bytes into a human-readable transcript. Compaction treats image data as payload size and summarizes it as an image attachment.

## Automatic path detection

A slash command is not required for image input. The CLI scans ordinary prompt text for existing local files. Examples:

```text
analyze this image workspace/pump-photo.png
```

Quoted paths are supported:

```text
analyze this image "/workspace/Pictures/Screenshot from 2026-01-08 23-42-07.png"
```

The detector also tries progressively longer token spans for unquoted paths containing spaces. Once a supported image is found, Crudo creates a user message containing both:

1. the original prompt text
2. the validated image block

The original prompt is preserved so the model receives the user's instruction together with the image.

Absolute paths outside the workspace are intentionally rejected by the current policy. To attach such a file, copy it into the active workspace or an approved workspace root first. This prevents ordinary image detection from becoming a way to bypass filesystem isolation.

## Ollama and Qwen3-VL setup

The intended local endpoint is:

```text
http://127.0.0.1:11434/v1
```

A typical environment is:

```bash
export OPENAI_BASE_URL=http://127.0.0.1:11434/v1
export OPENAI_API_KEY=ollama
```

Check installed models:

```bash
curl http://127.0.0.1:11434/api/tags
```

A model name observed during local verification was:

```text
qwen3-vl:4b-instruct
```

Run Crudo with the local model using the current local routing prefix:

```bash
cd rust
OPENAI_BASE_URL=http://127.0.0.1:11434/v1 \
OPENAI_API_KEY=ollama \
cargo run -p crudo-cli -- --model local/qwen3-vl:4b-instruct
```

Then enter an ordinary prompt containing a workspace-local image path:

```text
analyze this image demo/workspace/pump-photo.png
```

The local model server must be running and the model must be downloaded separately. Hardware, Ollama configuration, model loading time, and available memory determine whether inference completes promptly.

## Verification performed

The provider/API tests cover:

- supported image attachment validation
- workspace-boundary rejection
- unsupported file rejection
- content hashing
- image content block serialization
- OpenAI-compatible image payload construction
- existing provider and tool-message compatibility

The relevant Rust commands are:

```bash
scripts/fmt.sh
cd rust
cargo test -p api --lib
cargo check -p crudo-cli
```

The API test suite passed after the image block and validation changes. Workspace compilation also succeeded.

Ollama was reachable locally and reported a Qwen3-VL model. A direct image request was sent to the OpenAI-compatible Ollama endpoint. In the current environment, the inference request timed out while the model was processing; this is an environment/model-runtime limitation, not a request-format rejection.

## Current implementation status

Implemented:

- local attachment type and metadata contract
- workspace, type, and size validation
- BLAKE3 content hashing
- local-only attachment semantics
- image content blocks
- base64 image encoding
- OpenAI-compatible/Ollama image serialization
- runtime image blocks
- session persistence
- automatic image-path detection
- quoted and multi-word path detection
- compaction/export compatibility
- API and CLI compilation/tests

Not yet implemented:

- a bundled VLM
- automatic model installation or download
- guaranteed real-time Qwen3-VL inference
- multiple image attachments in one prompt
- direct PDF/document blocks for the vision model
- model capability negotiation and clear text-only-model rejection
- Gemini thought-signature preservation for tool-call history
- full CLI live-inference integration test
- vision-specific provenance and audit events

## Relationship to OCR and RAG

OCR remains a separate local capability. Scanned PDFs can be rendered and passed through PaddleOCR, with page-aware OCR JSON written into the workspace. The Rust RAG service recognizes `.ocr.json` files and indexes page labels such as:

```text
[OCR page 3]
Pump P-101 pressure is 12 bar
```

The multimodal image path is intended for visual reasoning, while OCR is intended for text extraction and RAG grounding:

```text
scanned PDF → OCR → local RAG → cited text evidence
image/P&ID  → local VLM → visual interpretation
```

OCR should not be represented as VLM reasoning, and a missing VLM must not be hidden behind an OCR-only fallback.

## Security expectations

In Sovereign Mode:

- attachment bytes originate from local validated files
- no external image URL is generated
- no cloud vision provider is selected by this attachment path
- local Ollama endpoints can remain loopback-only
- unsupported or invalid files are rejected before provider serialization
- external files outside the workspace require explicit copying into an approved root

A future VLM capability gate should require a local vision-capable model before sending image blocks. If the configured model is text-only, Crudo should return an explicit capability error rather than silently invoking a generic image tool or claiming visual understanding.
