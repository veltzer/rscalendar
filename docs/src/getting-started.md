# Getting Started

## First-Time Setup

After installing rscalendar and placing your OAuth2 credentials, authenticate:

```bash
rscalendar auth
```

This opens your browser for Google OAuth2 consent. The token is cached at `~/.config/rscalendar/token_cache.json` for future use.

If you're on a headless machine:

```bash
rscalendar auth --no-browser
```

This prints the auth URL instead of opening a browser.

## Basic Usage

List upcoming events:

```bash
rscalendar list
```

List more events:

```bash
rscalendar list --max-results 25
```

Create an all-day event:

```bash
rscalendar create --summary "Team offsite" --start 2026-04-01 --end 2026-04-02
```

Create a timed event:

```bash
rscalendar create --summary "Standup" --start "2026-04-01T09:00:00+03:00" --end "2026-04-01T09:15:00+03:00"
```

Delete an event by ID (shown in `list` output):

```bash
rscalendar delete --event-id <EVENT_ID>
```
