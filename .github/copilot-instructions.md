# Copilot Instructions for `rscalendar`

## Build, test, and lint commands

- Build: `cargo build`
- Run the CLI locally: `cargo run -- --help`
- Full test suite: `cargo test`
- Single test: `cargo test parses_rfc3339_time`
- Format: `cargo fmt`
- Formatting check: `cargo fmt --check`
- Lint: `cargo clippy --all-targets --all-features -- -D warnings`

## High-level architecture

- This repository is a Rust CLI for viewing and changing Google Calendar events.
- The current implementation is a single binary crate in `src/main.rs`.
- `clap` defines the command-line surface with four commands: `list`, `create`, `update`, and `delete`.
- `GoogleCalendarClient` wraps the direct REST calls to Google Calendar v3 using `reqwest` and a bearer token from the `GOOGLE_CALENDAR_ACCESS_TOKEN` environment variable.
- Request and response payloads are modeled with `serde` types in the same file, and `chrono` handles RFC3339 parsing plus date-only all-day event handling.
- `DESIGN.md` still defines the product boundary: keep the project focused on showing and changing Google Calendar data rather than expanding it into a broader calendar system.

## Key conventions

- Authentication is out of process in this first version. Do not add placeholder secrets or hardcoded tokens; the CLI expects `GOOGLE_CALENDAR_ACCESS_TOKEN` from the environment.
- Date parsing accepts either RFC3339 timestamps or `YYYY-MM-DD`. For all-day events, the CLI treats the supplied `--end` date as inclusive and converts it to Google Calendar's exclusive end-date format before sending the API request.
- `update` is intentionally sparse: only provided fields are patched, and calling it without any mutable fields should remain an error.
- The repository currently keeps the CLI, API client, payload helpers, and unit tests together in `src/main.rs`; preserve that simplicity unless growth makes a split clearly worthwhile.
