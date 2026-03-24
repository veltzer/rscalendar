# move-events

Move events from one calendar to another, optionally tagging each event with a shared extended property. Events are created in the target calendar and deleted from the source.

## Usage

```bash
rscalendar move-events --source <NAME> --target <NAME> [OPTIONS]
```

## Options

| Option | Required | Description |
|--------|----------|-------------|
| `--source <NAME>` | Yes | Source calendar name to move events from |
| `--target <NAME>` | Yes | Target calendar name to move events into |
| `--property-key <KEY>` | No | Shared extended property key to set on moved events |
| `--property-value <VALUE>` | No | Shared extended property value (requires `--property-key`) |
| `--interactive` | No | Prompt for each event: y=move, n=skip, q=quit |
| `--dry-run` | No | Show what would be done without making changes |

## Examples

Preview what would be moved:

```bash
rscalendar move-events --source "Client - John" --target Teaching --dry-run
```

Interactively move events, choosing which ones to move:

```bash
rscalendar move-events --source "Client - John" --target Teaching --interactive --property-key client --property-value John
```

Move all events without prompting:

```bash
rscalendar move-events --source "Client - John" --target Teaching --property-key client --property-value John
```
