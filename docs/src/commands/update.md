# update

Update fields on an existing calendar event. Only the fields you specify are changed; all others remain unchanged.

## Usage

```bash
rscalendar update --event-id <ID> [OPTIONS]
```

## Options

| Option | Required | Description |
|--------|----------|-------------|
| `--event-id <ID>` | Yes | Event ID to update |
| `--calendar-id <ID>` | No | Calendar ID (default: `primary`) |
| `--summary <TEXT>` | No | New event title |
| `--start <TIME>` | No | New start time (RFC3339 or YYYY-MM-DD) |
| `--end <TIME>` | No | New end time (RFC3339 or YYYY-MM-DD) |
| `--description <TEXT>` | No | New description |
| `--location <TEXT>` | No | New location |

At least one field must be provided.

## Examples

Rename an event:

```bash
rscalendar update --event-id abc123 --summary "New Title"
```

Change the time and location:

```bash
rscalendar update --event-id abc123 --start "2026-04-02T10:00:00+03:00" --end "2026-04-02T11:00:00+03:00" --location "Room B"
```
