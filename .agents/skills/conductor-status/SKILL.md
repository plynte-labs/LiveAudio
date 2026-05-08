---
name: conductor-status
description: Displays the current progress of the project
---


## 1.0 SYSTEM DIRECTIVE
You are an AI agent. Your primary function is to provide a status overview of the current tracks file. This involves reading the **Tracks Registry** file, parsing its content, and summarizing the progress of tasks.

CRITICAL: You must validate the success of every tool call. If any tool call fails, you MUST halt the current operation immediately, announce the failure to the user, and await further instructions.

## 1.0.1 LIVEAUDIO PROJECT OVERRIDES

- Tool mapping: use OpenCode `question` for `ask_user`, `apply_patch` for file edits, and `bash` for shell commands.
- Universal File Resolution Protocol for this project: resolve Product Definition to `conductor/product.md`, Product Guidelines to `conductor/product-guidelines.md`, Tech Stack to `conductor/tech-stack.md`, Workflow to `conductor/workflow.md`, Tracks Registry to `conductor/tracks.md`, and Tracks Directory to `conductor/tracks/`.
- Privacy rule: never save or expose raw audio, raw transcripts, session contents, secrets, API keys, private filesystem paths, or PII in Engram, prompts, logs, or review notes. Use sanitized technical summaries only.

---


## 1.1 SETUP CHECK
**PROTOCOL: Verify that the Conductor environment is properly set up.**

1.  **Verify Core Context:** Using the **Universal File Resolution Protocol**, resolve and verify the existence of:
    -   **Tracks Registry**
    -   **Product Definition**
    -   **Tech Stack**
    -   **Workflow**

2.  **Handle Failure:**
    -   If ANY of these files are missing, you MUST halt the operation immediately.
    -   Announce: "Conductor is not set up. Please run `/conductor:setup` to set up the environment."
    -   Do NOT proceed to Status Overview Protocol.

---

## 2.0 STATUS OVERVIEW PROTOCOL
**PROTOCOL: Follow this sequence to provide a status overview.**

### 2.1 Read Project Plan
1.  **Locate and Read:** Read the content of the **Tracks Registry** (resolved via **Universal File Resolution Protocol**).
2.  **Locate and Read Tracks:**
    -   Parse the **Tracks Registry** to identify all registered tracks and their paths.
        *   **Parsing Logic:** When reading the **Tracks Registry** to identify tracks, look for lines matching either the new standard format `- [ ] **Track:`, `- [~] **Track:`, `- [x] **Track:` or the legacy format `## [ ] Track:`, `## [~] Track:`, `## [x] Track:`.
    -   For each track with a valid track-folder link, resolve and read its **Implementation Plan** (using **Universal File Resolution Protocol** via the track's index file). For legacy completed tracks without a link, report them as completed historical entries and skip plan loading.

### 2.2 Parse and Summarize Plan
1.  **Parse Content:**
    -   Identify major project phases/sections (e.g., top-level markdown headings).
    -   Identify individual tasks and their current status (e.g., bullet points under headings, looking for keywords like "COMPLETED", "IN PROGRESS", "PENDING").
2.  **Generate Summary:** Create a concise summary of the project's overall progress. This should include:
    -   The total number of major phases.
    -   The total number of tasks.
    -   The number of tasks completed, in progress, and pending.

### 2.3 Present Status Overview
1.  **Output Summary:** Present the generated summary to the user in a clear, readable format. The status report must include:
    -   **Current Date/Time:** The current timestamp.
    -   **Project Status:** A high-level summary of progress (e.g., "On Track", "Behind Schedule", "Blocked").
    -   **Current Phase and Task:** The specific phase and task currently marked as "IN PROGRESS".
    -   **Next Action Needed:** The next task listed as "PENDING".
    -   **Blockers:** Any items explicitly marked as blockers in the plan.
    -   **Phases (total):** The total number of major phases.
    -   **Tasks (total):** The total number of tasks.
    -   **Progress:** The overall progress of the plan, presented as tasks_completed/tasks_total (percentage_completed%).
