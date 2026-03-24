# rscalendar - Google Calendar CLI Tool

A command-line tool for managing Google Calendar events, written in Rust.

## Features

- **List events** with flexible filtering by date and calendar
- **Create events** with support for all-day and timed events
- **Update events** by patching individual fields
- **Delete events** by ID
- **Shell completions** for bash, zsh, fish, and more

## Technology

- Built with Rust using direct Google Calendar API v3 calls via [reqwest](https://crates.io/crates/reqwest)
- OAuth2 authentication via [yup-oauth2](https://crates.io/crates/yup-oauth2)
- CLI powered by [clap](https://crates.io/crates/clap)
