# rscalendar

`rscalendar` is a small Rust CLI for viewing and changing Google Calendar events.

The first version is intentionally CLI-first. It talks directly to the Google Calendar REST API and uses a bearer token supplied through an environment variable instead of implementing OAuth login inside the app.

## Requirements

- Rust toolchain
- A Google OAuth access token with permission to manage calendar events

Export the token before running the app:

```bash
export GOOGLE_CALENDAR_ACCESS_TOKEN="your-access-token"
```

## Build, test, and lint

Build the application:

```bash
cargo build
```

Run the full test suite:

```bash
cargo test
```

Run a single test:

```bash
cargo test parses_rfc3339_time
```

Format the code:

```bash
cargo fmt
```

Check formatting without rewriting files:

```bash
cargo fmt --check
```

Run Clippy:

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

## Usage

Show the built-in CLI help:

```bash
cargo run -- --help
```

List upcoming events from the primary calendar:

```bash
cargo run -- list --max-results 5
```

Create a timed event:

```bash
cargo run -- create \
  --summary "Design review" \
  --start "2026-03-25T09:00:00+02:00" \
  --end "2026-03-25T10:00:00+02:00" \
  --location "Room 101"
```

Create an all-day event:

```bash
cargo run -- create \
  --summary "Vacation" \
  --start "2026-04-10" \
  --end "2026-04-12"
```

For all-day events, the CLI treats the `--end` date as inclusive and converts it to Google Calendar's exclusive end-date format.

Update an event:

```bash
cargo run -- update \
  --event-id "event-id" \
  --summary "Updated title"
```

Delete an event:

```bash
cargo run -- delete --event-id "event-id"
```

Use a non-primary calendar by adding `--calendar-id`.

## Architecture

The app currently lives in a single binary crate:

- `src/main.rs` contains the CLI definitions, the Google Calendar HTTP client, request payload helpers, and unit tests for date parsing and sparse patch generation.
- `clap` handles command parsing.
- `reqwest` performs authenticated REST calls.
- `serde` and `serde_json` map Google Calendar request and response payloads.
- `chrono` parses RFC3339 timestamps and date-only all-day events.

This version supports four operations:

- `list` for upcoming events
- `create` for inserting events
- `update` for patching selected event fields
- `delete` for removing events
