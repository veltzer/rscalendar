# event create

Create a new calendar event.

## Usage

```bash
rscalendar event create --summary <TEXT> --start <TIME> --end <TIME> [OPTIONS]
```

## Options

| Option | Required | Description |
|--------|----------|-------------|
| `--summary <TEXT>` | Yes | Event title |
| `--start <TIME>` | Yes | Start time (RFC3339 or YYYY-MM-DD) |
| `--end <TIME>` | Yes | End time (RFC3339 or YYYY-MM-DD) |
| `--calendar-id <ID>` | No | Calendar ID (default: `primary`) |
| `--description <TEXT>` | No | Event description |
| `--location <TEXT>` | No | Event location |

## Date Handling

- **Timed events**: Use RFC3339 format, e.g. `2026-04-01T09:00:00+03:00`
- **All-day events**: Use `YYYY-MM-DD` format. The end date is inclusive — rscalendar automatically converts it to Google's exclusive format.

## Examples

Create an all-day event:

```bash
rscalendar event create --summary "Holiday" --start 2026-04-01 --end 2026-04-01
```

Create a timed event with location:

```bash
rscalendar event create --summary "Lunch" --start "2026-04-01T12:00:00+03:00" --end "2026-04-01T13:00:00+03:00" --location "Cafe"
```
