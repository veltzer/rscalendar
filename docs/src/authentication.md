# Authentication

rscalendar uses OAuth2 to access the Google Calendar API on your behalf.

## How It Works

1. You provide OAuth2 client credentials (a JSON file from Google Cloud Console)
2. On first use, rscalendar opens your browser to get consent
3. The access token is cached locally for future requests

## Files

| File | Location | Purpose |
|------|----------|---------|
| Credentials | `~/.config/rscalendar/credentials.json` | OAuth2 client ID and secret |
| Token cache | `~/.config/rscalendar/token_cache.json` | Cached access/refresh tokens |

## Commands

Authenticate (opens browser):

```bash
rscalendar auth
```

Authenticate without browser (prints URL):

```bash
rscalendar auth --no-browser
```

Force re-authentication (removes cached token first):

```bash
rscalendar auth --force
```

## Scopes

rscalendar requests the `https://www.googleapis.com/auth/calendar` scope, which provides full read/write access to your Google Calendar.
