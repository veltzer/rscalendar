# list

List all events for a calendar.

## Usage

```bash
rscalendar list [OPTIONS]
```

## Options

| Option | Default | Description |
|--------|---------|-------------|
| `--calendar-name <NAME>` | from config | Calendar name to query |

## Examples

List all events from the default calendar:

```bash
rscalendar list
```

List all events from a specific calendar:

```bash
rscalendar list --calendar-name Teaching
```
