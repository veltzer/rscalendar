# migrate

Copy events from calendars matching a name prefix into a target calendar, tagging each event with the original client name as a shared extended property.

## Usage

```bash
rscalendar migrate --target <CALENDAR_ID> [OPTIONS]
```

## Options

| Option | Default | Description |
|--------|---------|-------------|
| `--target <ID>` | (required) | Target calendar ID to copy events into |
| `--prefix <PREFIX>` | `"Client - "` | Source calendar name prefix to match |
| `--dry-run` | `false` | Show what would be done without making changes |

## How It Works

1. Lists all calendars and filters those whose name starts with `--prefix` (default: `"Client - "`)
2. For each matching calendar, lists all events
3. Copies each event to the target calendar, preserving summary, start, end, description, and location
4. Sets the `client` shared extended property to the part of the calendar name after the prefix (e.g. for "Client - John", the `client` property is set to `"John"`)

## Examples

Preview what would happen:

```bash
rscalendar migrate --target abc123@group.calendar.google.com --dry-run
```

Run the migration:

```bash
rscalendar migrate --target abc123@group.calendar.google.com
```

Use a different prefix:

```bash
rscalendar migrate --target abc123@group.calendar.google.com --prefix "Student - "
```
