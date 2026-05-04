---
name: file-artifact-workflow
description: Используй этот skill для файлов, документов, Telegram documents, artifacts, deliver_file, attachments, downloads, generated files, screenshots, PDFs and session-scoped file delivery.
---

# File and Artifact Workflow

Use this skill when the operator sends, requests, edits, or receives files.

## File intake

- Telegram documents should be stored as session-scoped artifacts or approved workspace files.
- Confirm filename, size, storage location, and artifact id.
- Reject unsupported or oversized files with a clear explanation.

## File delivery

- If the user asks to receive a file, create or identify it first.
- Use `deliver_file` with either `workspace_path` or `artifact_id`.
- Treat `deliver_file` status `queued` as success; Telegram sends the document after the current turn.

## Artifacts

- Use artifacts for large tool outputs, generated files, screenshots, PDFs, diagnostics, and files that should be inspectable later.
- Use `artifact_read` or `artifact_search` only for known refs.
- Do not invent fallback delivery paths such as notes storage unless the user asks for that storage location.
