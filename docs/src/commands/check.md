# check

Check that events have all required properties based on their `type` property. The rules are defined in the `[check]` section of `config.toml`.

## Usage

```bash
rscalendar check [OPTIONS]
```

## Options

| Option | Required | Description |
|--------|----------|-------------|
| `--calendar-name <NAME>` | No | Calendar name (default: from config) |

## How It Works

1. Reads the `[check]` section from config, which maps each `type` value to a list of required property keys
2. For each event, looks up its `type` property
3. Verifies that all required properties for that type are present

## Reports

- Events missing a `type` property
- Events with a `type` value not defined in `[check]`
- Events missing required properties for their type

## Config

```toml
[check]
teaching = ["client", "company", "course"]
working = ["client", "company"]
call = ["client", "company"]
meeting = ["client", "company"]
```

This means: if an event has `type=teaching`, it must also have `client`, `company`, and `course` properties set.

## Example

```bash
$ rscalendar check
Linux Workshop (2026-04-01T09:00:00+03:00):
  - missing required property 'course' (required for type 'teaching')
Team Sync (2026-04-02T14:00:00+03:00):
  - missing 'type' property

2 issue(s) in 2 event(s) out of 10.
```
