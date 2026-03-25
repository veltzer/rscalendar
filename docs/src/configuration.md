# Configuration

rscalendar supports an optional TOML config file for setting defaults.

## File Location

```
~/.config/rscalendar/config.toml
```

The config file is optional. If missing, built-in defaults are used. All config values can be overridden by CLI flags.

## Options

| Key | Type | Default | Description |
|-----|------|---------|-------------|
| `calendar_name` | string | (none) | Default calendar name for all commands |
| `no_browser` | boolean | `false` | Don't open browser during `auth` |

## The `[check]` Section

The `[check]` section defines required properties per event type. It is used by the `rscalendar check` command to validate that events have all expected properties.

```toml
[check]
teaching = ["client", "company", "course"]
working = ["client", "company"]
call = ["client", "company"]
meeting = ["client", "company"]
```

Each key is a `type` property value; its array lists the property keys that must be present on events of that type.

## Example

```toml
# Use a specific calendar by default
calendar_name = "Teaching"

# Always print URL instead of opening browser (headless machines)
no_browser = true

# Required properties per event type
[check]
teaching = ["client", "company", "course"]
working = ["client", "company"]
```

## Precedence

CLI flags always win over config file values:

```bash
# Uses calendar_name from config.toml
rscalendar list

# Overrides config.toml with the flag value
rscalendar list --calendar-name "Work"
```

## Files Overview

| File | Purpose |
|------|---------|
| `~/.config/rscalendar/config.toml` | User preferences |
| `~/.config/rscalendar/credentials.json` | OAuth2 client credentials |
| `~/.config/rscalendar/token_cache.json` | Cached access/refresh tokens |
