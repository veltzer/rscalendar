# Coding Style

## Output formatting

- Do not use leading spaces for indentation in printed output. All `println!`, `eprintln!`, `print!`, `eprint!` format strings should start at column 0 (no `"  "` prefix).
- Use `---` as a separator between built-in and custom properties, not indented.

## Build and test

- `cargo build` must produce no warnings.
- `cargo test` must pass all tests.
- `mdbook build docs` must succeed.
