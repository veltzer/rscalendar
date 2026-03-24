# list

List upcoming events for a calendar.

## Usage

```bash
rscalendar list [OPTIONS]
```

## Options

| Option | Default | Description |
|--------|---------|-------------|
| `--calendar-id <ID>` | `primary` | Calendar ID to query |
| `--max-results <N>` | `10` | Number of events to return |
| `--time-min <RFC3339>` | now | Lower bound for event start times |
| `--show-deleted` | `false` | Include deleted events |

## Examples

List next 10 events:

```bash
rscalendar list
```

List next 50 events:

```bash
rscalendar list --max-results 50
```

List events starting from a specific date:

```bash
rscalendar list --time-min "2026-04-01T00:00:00Z"
```
