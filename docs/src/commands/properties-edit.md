# properties edit

Interactively edit properties on each event via TUI menus. For each event, you can add, change, or delete properties in a loop before moving to the next event.

## Usage

```bash
rscalendar properties edit [OPTIONS]
```

## Options

| Option | Required | Description |
|--------|----------|-------------|
| `--calendar-name <NAME>` | No | Calendar name (default: from config) |

## How It Works

For each event, the TUI shows:

1. The event name and start time
2. Current properties
3. A numbered menu of actions:
   - **add** — for properties defined in config but not set on the event
   - **change** — for properties already set (pick a new value from the allowed list)
   - **delete** — remove a property from the event
   - **n** — save changes and move to the next event
   - **q** — save changes and quit

Changes are saved when you press `n` (next) or `q` (quit). If no changes were made, nothing is sent to the API.

## Example Session

```
Event: Linux Workshop (2026-04-01T09:00:00+03:00)
  company: AgileSparks
  Actions:
    1: change 'company'
    2: delete 'company'
    3: add 'course'
    n: next event
    q: quit
  choice: 3
  Select course:
    1: Linux Fundamentals
    2: Advanced Python
    s: skip this property
  choice: 1
  company: AgileSparks
  course: Linux Fundamentals
  Actions:
    1: change 'company'
    2: delete 'company'
    3: change 'course'
    4: delete 'course'
    n: next event
    q: quit
  choice: n
  saved.
```

## Config

Property keys and allowed values must be defined in `~/.config/rscalendar/config.toml`:

```toml
[properties]
company = ["AgileSparks", "Intel", "Google"]
course = ["Linux Fundamentals", "Advanced Python"]
```
