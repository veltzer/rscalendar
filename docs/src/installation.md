# Installation

## Prerequisites

- Rust toolchain (edition 2024)
- Google Cloud project with Calendar API enabled
- OAuth2 credentials (Desktop application type)

## Install from crates.io

```bash
cargo install rscalendar
```

This downloads, compiles, and installs the latest published version into `~/.cargo/bin/`.

## Building from Source

```bash
git clone https://github.com/veltzer/rscalendar.git
cd rscalendar
cargo build --release
```

The binary will be at `target/release/rscalendar`.

## Google Cloud Setup

1. Go to [Google Cloud Console](https://console.cloud.google.com/)
2. Create a new project (or use an existing one)
3. Enable the **Google Calendar API** under APIs & Services
4. Create OAuth2 credentials:
   - Go to APIs & Services > Credentials
   - Click "Create Credentials" > "OAuth client ID"
   - Choose "Desktop application" as the application type
   - Download the JSON file
5. Place the credentials file at `~/.config/rscalendar/credentials.json`
