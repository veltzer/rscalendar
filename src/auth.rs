use std::future::Future;
use std::pin::Pin;

use anyhow::{Context, Result};

use crate::config::{Config, credentials_path, token_cache_path};

const CALENDAR_SCOPE: &str = "https://www.googleapis.com/auth/calendar";

pub struct NoInteractionDelegate;

impl yup_oauth2::authenticator_delegate::InstalledFlowDelegate for NoInteractionDelegate {
    fn present_user_url<'a>(
        &'a self,
        _url: &'a str,
        _need_code: bool,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>> {
        Box::pin(async move {
            Err("cached token is expired or revoked and cannot be refreshed without a browser. Run 'rscalendar auth --force'.".to_string())
        })
    }
}

/// Build an authenticator that never starts a browser sign-in.
///
/// A valid cached access token is used as is and an expired one is refreshed;
/// when that refresh fails, yup-oauth2 falls back to the installed flow, and
/// [`NoInteractionDelegate`] refuses it so the caller gets an error.
///
/// The return method must be `Interactive`: with `HTTPRedirect` yup-oauth2
/// ignores the delegate's refusal and waits forever for a browser callback,
/// so every command hung once the refresh token expired.
pub async fn noninteractive_authenticator(
    credentials: &std::path::Path,
    cache: &std::path::Path,
) -> Result<yup_oauth2::authenticator::DefaultAuthenticator> {
    let secret = yup_oauth2::read_application_secret(credentials)
        .await
        .context("failed to read credentials.json")?;
    yup_oauth2::InstalledFlowAuthenticator::builder(
        secret,
        yup_oauth2::InstalledFlowReturnMethod::Interactive,
    )
    .persist_tokens_to_disk(cache)
    .flow_delegate(Box::new(NoInteractionDelegate))
    .build()
    .await
    .context("failed to build authenticator")
}

pub struct BrowserFlowDelegate;

impl yup_oauth2::authenticator_delegate::InstalledFlowDelegate for BrowserFlowDelegate {
    fn present_user_url<'a>(
        &'a self,
        url: &'a str,
        _need_code: bool,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>> {
        Box::pin(async move {
            if let Err(e) = open::that(url) {
                eprintln!(
                    "Failed to open browser: {}. Please open this URL manually:\n{}",
                    e, url
                );
            }
            Ok(String::new())
        })
    }
}

pub async fn cmd_auth(no_browser_flag: bool, force: bool, config: &Config) -> Result<()> {
    if force {
        let cache = token_cache_path()?;
        if cache.exists() {
            std::fs::remove_file(&cache)?;
            eprintln!("Removed cached token at {}", cache.display());
        }
    }

    let secret = yup_oauth2::read_application_secret(credentials_path()?)
        .await
        .context("failed to read credentials.json")?;

    let mut builder = yup_oauth2::InstalledFlowAuthenticator::builder(
        secret,
        yup_oauth2::InstalledFlowReturnMethod::HTTPRedirect,
    )
    .persist_tokens_to_disk(token_cache_path()?);

    let no_browser = no_browser_flag || config.no_browser();
    if !no_browser {
        builder = builder.flow_delegate(Box::new(BrowserFlowDelegate));
    }

    let auth = builder
        .build()
        .await
        .context("failed to build authenticator")?;

    let _token = auth
        .token(&[CALENDAR_SCOPE])
        .await
        .context("failed to obtain token")?;

    eprintln!(
        "Authentication successful. Token cached to {}",
        token_cache_path()?.display()
    );
    Ok(())
}

/// Check that the cached token is usable without any user interaction.
///
/// Loads the token cache and asks for an access token with the
/// non-interactive delegate: a still-valid access token is returned as is, an
/// expired one is refreshed with the refresh token (which also counts as use
/// of the OAuth client in Google's inactivity policy). A missing cache or a
/// rejected refresh token is reported and exits with status 1.
pub async fn cmd_auth_check_token() -> Result<()> {
    let cache_path = token_cache_path()?;
    if !cache_path.exists() {
        eprintln!(
            "Token: MISSING ({}). Run 'rscalendar auth'.",
            cache_path.display()
        );
        std::process::exit(1);
    }
    let auth = noninteractive_authenticator(&credentials_path()?, &cache_path).await?;
    match auth.token(&[CALENDAR_SCOPE]).await {
        Ok(token) => {
            match token.expiration_time() {
                Some(exp) => println!("Token: VALID (access token expires {exp})"),
                None => println!("Token: VALID"),
            }
            Ok(())
        }
        Err(yup_oauth2::Error::UserError(_)) => {
            eprintln!(
                "Token: INVALID (could not refresh: refresh token expired or revoked, or Google unreachable). Run 'rscalendar auth --force'."
            );
            std::process::exit(1);
        }
        Err(e) => Err(e).context("failed to check token"),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    /// Regression test: a failed token refresh must produce an error, not a
    /// hang. The token endpoint is a closed local port, so the refresh always
    /// fails and the test runs offline.
    #[tokio::test]
    async fn failed_refresh_errors_instead_of_hanging() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
        let dir = std::env::temp_dir().join(format!("rscalendar-auth-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let credentials = dir.join("credentials.json");
        let cache = dir.join("token_cache.json");
        std::fs::write(
            &credentials,
            r#"{"installed":{"client_id":"id","client_secret":"secret",
                "auth_uri":"https://127.0.0.1:1/auth","token_uri":"https://127.0.0.1:1/token",
                "redirect_uris":["http://localhost"]}}"#,
        )
        .unwrap();
        std::fs::write(
            &cache,
            r#"[{"scopes":["https://www.googleapis.com/auth/calendar"],
                "token":{"access_token":"expired","refresh_token":"refresh",
                "expires_at":[2020,1,0,0,0,0,0,0,0],"id_token":null}}]"#,
        )
        .unwrap();

        let auth = noninteractive_authenticator(&credentials, &cache)
            .await
            .unwrap();
        let result =
            tokio::time::timeout(Duration::from_secs(10), auth.token(&[CALENDAR_SCOPE])).await;
        std::fs::remove_dir_all(&dir).unwrap();

        let outcome = result.expect("token() hung instead of failing");
        assert!(
            matches!(outcome, Err(yup_oauth2::Error::UserError(_))),
            "expected the delegate's refusal, got {outcome:?}"
        );
    }
}
