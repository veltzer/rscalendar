# auth

Authenticate with Google via OAuth2 and cache the token locally.

## Usage

```bash
rscalendar auth [OPTIONS]
```

## Options

| Option | Description |
|--------|-------------|
| `--no-browser` | Print the authorization URL instead of opening the browser |
| `--force` | Remove cached token and force re-authentication |

## Examples

Normal authentication (opens browser):

```bash
rscalendar auth
```

Headless authentication:

```bash
rscalendar auth --no-browser
```

Re-authenticate:

```bash
rscalendar auth --force
```
