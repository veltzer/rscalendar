# Authentication

rscalendar uses OAuth2 to access the Google Calendar API on your behalf.

## How It Works

1. You provide OAuth2 client credentials (a JSON file from Google Cloud Console)
2. On first use, rscalendar opens your browser to get consent
3. The access token is cached locally for future requests

## Where the secrets live

rscalendar looks in [pass](https://www.passwordstore.org/) first and falls back
to plain files when `pass` is not installed or the entry does not exist.

| What | pass entry (default) | Fallback file |
|------|----------------------|---------------|
| OAuth2 client (the JSON from Google Cloud Console) | `gcp/veltzer-rscalendar-id/oauth-client` | `~/.config/rscalendar/credentials.json` |
| Token cache (access/refresh tokens) | `gcp/veltzer-rscalendar-id/token` | `~/.config/rscalendar/token_cache.json` |

Users without a pass store need do nothing: the files work exactly as before.

The entries can be renamed in `config.toml` (see [Configuration](configuration.md)).

To use pass, store the content of the two files in the entries:

```bash
pass insert -m gcp/veltzer-rscalendar-id/oauth-client < credentials.json
pass insert -m gcp/veltzer-rscalendar-id/token < token_cache.json
```

The pass token entry has the same JSON layout as `token_cache.json`.

Rules in pass mode:

- The token entry is used when it exists, or when the OAuth client came from
  pass. In that case `rscalendar auth` creates the token entry and never
  writes a plaintext token file.
- The pass store is usually a git repository, so the token entry is not
  rewritten every time the one-hour access token is refreshed. Refreshed
  access tokens are kept in memory only. The entry is written when you
  authorize (`auth`, `auth --force`) or when Google issues a new refresh token.
- `auth --force` removes the token entry (`pass rm`) before authorizing again.
- Secret values are never printed; messages name the entry only.

## Commands

Authenticate (opens browser):

```bash
rscalendar auth
```

Authenticate without browser (prints URL):

```bash
rscalendar auth --no-browser
```

Force re-authentication (removes the cached token first, in pass or on disk):

```bash
rscalendar auth --force
```

Check whether the cached token still works, without opening a browser
(refreshes the access token if it has expired; exits 1 if the refresh token
is missing, expired or revoked):

```bash
rscalendar auth check-token
```

## Scopes

rscalendar requests the `https://www.googleapis.com/auth/calendar` scope, which provides full read/write access to your Google Calendar.
