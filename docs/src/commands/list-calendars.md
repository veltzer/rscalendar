# list-calendars

List all calendars accessible to the authenticated user.

## Usage

```bash
rscalendar list-calendars
```

## Output

For each calendar, shows:

- **Name** (with `(primary)` marker for the default calendar)
- **id** — the calendar ID to use with `--calendar-id` or in `config.toml`
- **role** — your access level (`owner`, `writer`, `reader`, `freeBusyReader`)
- **description** — calendar description, if set

## Example

```bash
$ rscalendar list-calendars
My Calendar (primary)
  id: user@gmail.com
  role: owner

Work
  id: abc123@group.calendar.google.com
  role: writer

Holidays in Israel
  id: en.jewish#holiday@group.v.calendar.google.com
  role: reader
```

Use the `id` value as `--calendar-id` or set it as `calendar_id` in `~/.config/rscalendar/config.toml`.
