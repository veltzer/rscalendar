# properties add

Add shared extended properties to events. Validates keys and values against the `[properties]` section in `config.toml`.

## Usage

```bash
rscalendar properties add [OPTIONS]
```

## Options

| Option | Required | Description |
|--------|----------|-------------|
| `--calendar-name <NAME>` | No | Calendar name (default: from config) |
| `--key <KEY>` | No | Property key to set (must be in config). If omitted, prompts for all missing properties |
| `--value <VALUE>` | No | Property value (must be allowed for the key). Requires `--key` |
| `--all` | No | Apply to all events without prompting |

## Modes

### Interactive (no --key/--value)

Walks each event and prompts for all missing properties using a numbered menu of allowed values from config:

```bash
rscalendar properties add
```

### Single property (--key and --value)

Sets a specific property, prompting per event (y/n/q):

```bash
rscalendar properties add --key company --value AgileSparks
```

Skip prompting with `--all`:

```bash
rscalendar properties add --key company --value AgileSparks --all
```

## Config

Properties must be defined in `~/.config/rscalendar/config.toml`:

```toml
[properties]
company = ["AgileSparks", "Intel", "Google"]
course = ["Linux Fundamentals", "Advanced Python"]
```
