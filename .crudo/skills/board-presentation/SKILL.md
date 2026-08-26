---
name: board-presentation
description: Create an evidence-bound editable board presentation using the local industrial demo tools.
---

# Board presentation workflow

For synthetic/public demo requests involving a board presentation:

1. Classify the request and state that assumptions are synthetic unless source files are provided.
2. For a board-deck request without source files, call `mcp__industrial_demo__create_board_presentation` directly. Do not inspect the MCP server source, list implementation files, or use Bash to construct the deck.
3. Use the `industrial_demo` MCP tools for document extraction, OCR, and local knowledge search when source inputs exist.
4. Produce a structured, evidence-bound presentation plan when a custom deck is required.
5. Call `mcp__industrial_demo__generate_presentation` with the plan path. Never use Bash to write PPTX XML or construct a presentation manually.
6. Call `mcp__industrial_demo__verify_presentation` on the generated artifact.
7. Report the artifact path, verification checks, citations, assumptions, and `human_review_required: true`.

Do not send confidential files to the remote Luna development endpoint. Do not claim OCR, RAG, vision, rendering, or sandbox execution succeeded unless the corresponding tool returned an active/success status. A presentation generated from synthetic assumptions must be clearly labeled as a demo and not an operational recommendation.
