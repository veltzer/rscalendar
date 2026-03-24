# Copilot Instructions for `rscalendar`

## Repository state and source of truth

- The repository is currently at a very early stage. The committed tree contains only `README.md`, and the current worktree also includes a staged `DESIGN.md`.
- `README.md` is currently empty.
- Treat `DESIGN.md` as the current product-definition document. It defines the intended scope as a Rust application that shows and changes data in Google Calendar.

## Build, test, and lint commands

- No build, test, or lint commands are defined in the repository yet.
- There is no committed `Cargo.toml`, `src/`, CI workflow, or task runner at the moment, so do not assume `cargo build`, `cargo test`, or lint targets exist until the project is bootstrapped.
- When adding automation later, prefer documenting the exact repository-local commands here instead of inventing wrapper scripts.

## High-level architecture

- The current architecture is documentation-first rather than code-first.
- The only established application boundary is from `DESIGN.md`: this project is intended to be a Rust application that reads from and writes to Google Calendar.
- Before implementing features, keep changes aligned with that narrow scope instead of expanding into a general calendar platform.

## Key conventions

- Use the repository docs as the authoritative description of behavior until source code exists.
- Keep future implementation choices consistent with the current stated stack: Rust for the application code and Google Calendar as the integration target.
- Because the repository is not bootstrapped yet, future Copilot sessions should inspect the working tree before assuming standard Rust layout or commands.
