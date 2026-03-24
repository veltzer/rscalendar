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

## Example

```toml
# Use a specific calendar by default
calendar_name = "Teaching"

# Always print URL instead of opening browser (headless machines)
no_browser = true
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
