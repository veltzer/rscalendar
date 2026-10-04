# TOFIX

Findings from a code scan on 2026-10-04.

## High

- `scripts/migrate.sh:2` - calls `rscalendar move-events` with `--property-key` and `--property-value`, but `MoveEventsArgs` (`src/cli.rs:152`) only has `--source`, `--target`, `--dry-run` and `--all`, so clap rejects the command; either add property filtering to `move-events` or fix/delete the script.
- `src/commands/move_events.rs:94` - move rebuilds each event from only summary/start/end/description/location/extendedProperties and then deletes the original (`:112`), silently losing attendees, reminders, colour, conference data, and recurrence (events are fetched with `singleEvents=true` at `src/client.rs:164`, so a recurring series is flattened into standalone copies); use the Calendar API `events.move` endpoint, which keeps the event intact. `cmd_calendar_copy` (`src/commands/calendar.rs:194`) has the same field loss.

## Medium

- `src/models.rs:29` - `html_link` has no `#[serde(rename = "htmlLink")]` (the struct has no `rename_all`), so the API's `htmlLink` field is never deserialised and the `link:` output at `src/models.rs:85` and `:152` never appears; add the rename.
- `src/client.rs:221` - `list_calendars` makes a single request and ignores `nextPageToken`, so users with more calendars than one page (default 100) get a truncated list and `resolve_calendar_id` reports "no calendar named ..." for calendars that exist; paginate as `list_all_events` does.
- `README.md:13` - the whole README describes an old version: auth via `GOOGLE_CALENDAR_ACCESS_TOKEN` (the code uses OAuth with `rscalendar auth` and a token cache, `src/client.rs:24`), top-level `create`/`update`/`delete` commands (now `event create|update|delete`), `list --max-results` (no such flag), and a single-file `src/main.rs` architecture; rewrite it to match the current CLI or point to the mdbook in `docs/`.
- `.github/copilot-instructions.md:15` - same stale description (single `src/main.rs`, four commands, `GOOGLE_CALENDAR_ACCESS_TOKEN` env var, "keep everything in main.rs"); this actively steers AI assistants to wrong changes; update or delete it.
- `docs/src/commands/move-events.md:17` - documents an `--interactive` flag that does not exist and says the command without flags moves all events without prompting; the code is the opposite (prompts by default, `--all` skips prompts, `src/cli.rs:163`); fix the options table and examples.

## Low

- `docs/src/SUMMARY.md:7` - commands `stats`, `version`, `calendar delete|rename|clear|copy`, `event edit` and `properties set-value` (`src/cli.rs:31-129`) have no doc pages, and `docs/src/commands.md:24` omits the `--quiet` global flag; add them.
- `docs/src/commands/list.md:121` - lists only `--calendar-name`, but `list` also takes `--starts-after`, `--starts-before`, `--search`, `--has-property`, `--count` and `--format` (`src/cli.rs:263-286`); document them.
- `docs/src/configuration.md:84` - the `[properties]` section is not documented although `[check]` keys are validated against it (`src/config.rs:49-60`), so the example at `:114-118` produces "[check] references property ... not defined" warnings; document `[properties]` and include it in the example. The `defconfig` template has the same gap: `[check]` requires `client` (`src/main.rs:79`) but `[properties]` defines no `client` key.
- `src/commands/move_events.rs:15` - source/target calendars are resolved with `.find` (first name match) instead of `resolve_calendar_id`, which rejects ambiguous names (`src/client.rs:457`); with duplicate calendar names this silently picks one. Same in `src/commands/calendar.rs:138`.
- `src/client.rs:470` - `api_error` discards the response body, so every API failure is just "request failed with status 400" with no Google error message; read and include the error JSON's `error.message`.
- `DESIGN.md:1` - two-line stub with a grammar error ("This app show and allows ...") that says nothing about the design; flesh it out or remove it (`docs/src/design-decisions.md` already exists).
