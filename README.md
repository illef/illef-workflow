# illef-workflow

A lightweight personal workflow scheduler. Built to address the pain points of cron and systemd timers.

## Motivation

- **crontab**: No visibility into whether jobs ran, succeeded, or failed. Essentially a black box.
- **systemd timer**: Adding a workflow requires creating a `.service` file, a `.timer` file, then running `enable`, `start`, and `daemon-reload`. Too much friction.

illef-workflow manages all workflows through a single `config.yaml` file and lets you inspect execution status and logs instantly via a TUI.

## Components

### Runner

- Runs as a systemd user service in the background
- Reads `config.yaml` and executes scripts according to their cron schedules (local time)
- Detects `config.yaml` changes via inotify and reloads without restart
- Handles TUI requests over a Unix domain socket + gRPC
- Sends notifications on success/failure via `notify-send` (configurable)
  - title: `{workflow name} succeeded / failed`
  - body: stdout of `message_script`, or a default message if not set

### TUI

- Connects to the Runner via gRPC and supports:
  - Listing registered workflows (with cron expression and time until next run)
  - Viewing execution history for a workflow
  - Reading execution log files directly (`tail -f` style)
  - Triggering a workflow immediately

## Installation

```bash
make install
systemctl --user enable --now illef-workflow
```

To uninstall:

```bash
make uninstall
```

To update the TUI binary only:

```bash
make install:tui
```

## File Structure

```
~/.config/illef-workflow/
├── config.yaml          # workflow definitions
└── scripts/             # scripts to execute

~/.cache/illef-workflow/
├── logs/
│   └── {workflow_name}/
│       └── {execution_id}.log
└── storage.sqlite       # persistent execution history

/tmp/illef-workflow.sock  # Unix domain socket (Runner ↔ TUI IPC)
```

## config.yaml

Cron expressions support both the standard 5-field format (`min hour day month weekday`) and the 6-field format (`sec min hour day month weekday`). Schedules are evaluated in **local time**.

```yaml
workflows:
  - name: daily-backup
    cron: "0 3 * * *"          # every day at 03:00
    script: backup.sh
    message_script: backup_message.sh  # optional

  - name: weekly-cleanup
    cron: "0 0 * * 0"          # every Sunday at midnight
    script: cleanup.sh

notifications:
  command: notify-send
```

`message_script` is optional and only runs on success. Its stdout becomes the notification body. If omitted or if the script exits with code 3, the notification is suppressed.

### message_script exit codes

| Exit code | Behavior |
|-----------|----------|
| `0` | Send notification with stdout as body |
| `3` | Suppress notification (nothing to report) |
| other | Send notification with default body |

## TUI Layout

```
┌─────────────────────┬─────────────────────────────────────────────────┐
│ Workflows           │ {workflow} - Executions                         │
│ ○ daily-backup      │ ✓ 02-27 03:00                                   │
│   0 3 * * *  in 8h  │ ✗ 02-26 03:00                                   │
│                     │                                                 │
├─────────────────────┴─────────────────────────────────────────────────┤
│ Log - 2026-02-27 03:00                                                │
│ [2026-02-27 03:00:00] Starting workflow: daily-backup                 │
│ [2026-02-27 03:00:01] Finished with exit code: 0                      │
├───────────────────────────────────────────────────────────────────────┤
│ [←→] switch panel  [w] workflows  [↑↓] select  [r] run now  [q] quit  │
└───────────────────────────────────────────────────────────────────────┘
```

## TUI Keybindings

| Key | Action |
|-----|--------|
| `→` | Move to Executions panel |
| `←` | Move to Workflows panel |
| `Tab` | Cycle to next panel |
| `w` | Jump to Workflows panel |
| `↑` / `k` | Select previous item |
| `↓` / `j` | Select next item |
| `Enter` | Confirm selection and move to next panel |
| `r` | Trigger selected workflow immediately |
| `q` | Quit |

## Design Decisions

| Decision | Choice | Reason |
|----------|--------|--------|
| IPC | Unix socket + gRPC | Type-safe API |
| State storage | SQLite | Lightweight, no separate daemon needed |
| Log viewing | TUI reads file directly after receiving path | Simple, no gRPC streaming needed |
| Queue | In-memory, lost on shutdown | Simplicity first |
| Workflow identity | Name-based, overwrite on change | No versioning complexity |
| Hot-reload | inotify-based | Reflects changes without restart |
| Notifications | notify-send (default) | Configurable via config.yaml |
| Cron timezone | Local time | Matches user expectation |
| Language | Rust | Single binary, suitable for long-running daemon |

## Concurrency Policy

- If a workflow is already running when a trigger request arrives, the request is queued in memory
- The queue is lost when the Runner stops

## Config Change Handling

- Workflow deleted: past execution history is preserved, no further executions
- Schedule or script changed: the workflow is overwritten and the new schedule takes effect immediately
