# Coding Style

## Output formatting

- Do not use leading spaces for indentation in printed output. All `println!`, `eprintln!`, `print!`, `eprint!` format strings should start at column 0 (no `"  "` prefix).
- Use `---` as a separator between built-in and custom properties, not indented.

## Build and test

- `cargo build` must produce no warnings.
- `cargo test` must pass all tests.
- `mdbook build docs` must succeed.

## Credentials

- The OAuth client JSON and the token cache come from pass(1) first (`gcp/veltzer-rscalendar-id/oauth-client` and `gcp/veltzer-rscalendar-id/token`, overridable in the `[pass]` table of config.toml), and fall back to `~/.config/rscalendar/credentials.json` and `token_cache.json` when `pass` or the entry is missing. The code is in `src/pass.rs` and `src/auth.rs`.
- GCP project: `veltzer-rscalendar-id`. The repo's `.gcp.conf` names it in `gcp_project`, and bash-bashy's `prompt_gcp` activates the gcloud configuration of the same name on `cd` into the repo.
- Never write refreshed access tokens to pass: the store is git-tracked. Only a new authorization or a changed refresh token is written.
- Never log secret values; messages may name the entry only.
