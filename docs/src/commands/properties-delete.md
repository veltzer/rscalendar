# properties delete

Delete a shared extended property from events.

## Usage

```bash
rscalendar properties delete --key <KEY> [OPTIONS]
```

## Options

| Option | Required | Description |
|--------|----------|-------------|
| `--key <KEY>` | Yes | Property key to delete |
| `--calendar-name <NAME>` | No | Calendar name (default: from config) |
| `--all` | No | Apply to all events without prompting |

## Details

Interactive by default — prompts (y/n/q) for each event that has the property. Events that don't have the property are skipped automatically.

The deletion works by sending the property key with a `null` value to the Google Calendar API, which is the only way to remove an extended property.

## Examples

Delete interactively:

```bash
rscalendar properties delete --key client
```

Delete from all events without prompting:

```bash
rscalendar properties delete --key client --all
```
