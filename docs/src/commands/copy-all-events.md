# copy-all-events

Copy all events from one calendar to another, optionally tagging each copied event with a shared extended property.

## Usage

```bash
rscalendar copy-all-events --source <NAME> --target <NAME> [OPTIONS]
```

## Options

| Option | Required | Description |
|--------|----------|-------------|
| `--source <NAME>` | Yes | Source calendar name to copy events from |
| `--target <NAME>` | Yes | Target calendar name to copy events into |
| `--property-key <KEY>` | No | Shared extended property key to set on copied events |
| `--property-value <VALUE>` | No | Shared extended property value (requires `--property-key`) |
| `--dry-run` | No | Show what would be done without making changes |

## Examples

Preview a migration:

```bash
rscalendar copy-all-events --source "Client - John" --target Teaching --dry-run
```

Copy events and tag them:

```bash
rscalendar copy-all-events --source "Client - John" --target Teaching --property-key client --property-value John
```

Copy without tagging:

```bash
rscalendar copy-all-events --source "Old Calendar" --target "New Calendar"
```
