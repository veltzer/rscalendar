# event delete

Delete a calendar event by ID.

## Usage

```bash
rscalendar event delete --event-id <ID> [OPTIONS]
```

## Options

| Option | Required | Description |
|--------|----------|-------------|
| `--event-id <ID>` | Yes | Event ID to delete |
| `--calendar-id <ID>` | No | Calendar ID (default: `primary`) |

## Examples

```bash
rscalendar event delete --event-id abc123
```

The event ID can be found in the output of `rscalendar list`.
