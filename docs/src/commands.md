# Commands

rscalendar provides the following commands:

| Command | Description |
|---------|-------------|
| [list-calendars](commands/list-calendars.md) | List all accessible calendars |
| [list](commands/list.md) | List all events for a calendar |
| [create](commands/create.md) | Create a new event |
| [update](commands/update.md) | Update fields on an existing event |
| [delete](commands/delete.md) | Delete an event |
| [calendar create](commands/calendar-create.md) | Create a new public calendar |
| [properties add](commands/properties-add.md) | Add properties to events |
| [properties check](commands/properties-check.md) | Validate event properties against config |
| [properties delete](commands/properties-delete.md) | Delete a property from events |
| [properties rename](commands/properties-rename.md) | Rename a property key on events |
| [move-events](commands/move-events.md) | Move events between calendars |
| [auth](commands/auth.md) | Authenticate with Google |
| [defconfig](commands/defconfig.md) | Print default configuration |
| [complete](commands/complete.md) | Generate shell completions |

## Global Flags

| Flag | Description |
|------|-------------|
| `--show-builtin` | Show "(built-in)" labels on standard Google Calendar fields |
| `--json` | Output as JSON for processing with tools like `jq` |

All commands that interact with the calendar require prior authentication via `rscalendar auth`.
