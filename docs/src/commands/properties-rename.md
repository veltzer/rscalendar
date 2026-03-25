# properties rename

Rename a shared extended property key on events. The value is preserved.

## Usage

```bash
rscalendar properties rename --from <OLD_KEY> --to <NEW_KEY> [OPTIONS]
```

## Options

| Option | Required | Description |
|--------|----------|-------------|
| `--from <OLD_KEY>` | Yes | Current property key name |
| `--to <NEW_KEY>` | Yes | New property key name |
| `--calendar-name <NAME>` | No | Calendar name (default: from config) |
| `--all` | No | Apply to all events without prompting |

## Details

Interactive by default — prompts (y/n/q) for each event that has the old key. Events without the old key are skipped.

The rename removes the old key and adds the new key with the same value in a single PATCH request.

## Examples

Rename interactively:

```bash
rscalendar properties rename --from source_calendar --to client
```

Rename on all events without prompting:

```bash
rscalendar properties rename --from source_calendar --to client --all
```
