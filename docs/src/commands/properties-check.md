# properties check

Check that all event properties have keys and values defined in the `[properties]` section of `config.toml`.

## Usage

```bash
rscalendar properties check [OPTIONS]
```

## Options

| Option | Required | Description |
|--------|----------|-------------|
| `--calendar-name <NAME>` | No | Calendar name (default: from config) |

## What It Checks

- **Missing properties** — keys defined in config but not present on the event
- **Invalid values** — property values not in the allowed list for that key
- **Unknown properties** — keys on the event that are not defined in config

## Example

```bash
$ rscalendar properties check
Team Meeting (2026-04-01T10:00:00+03:00):
  - missing property 'course'
Workshop (2026-04-02T09:00:00+03:00):
  - property 'company' has value 'Foo' which is not in allowed values: AgileSparks, Intel, Google

2 issue(s) found across 5 event(s).
```
