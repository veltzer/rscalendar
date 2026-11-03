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
        Box::pin(async move { Err("Not authenticated. Run 'rscalendar auth' first.".to_string()) })
    }
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
    let secret = yup_oauth2::read_application_secret(credentials_path()?)
        .await
        .context("failed to read credentials.json")?;
    let auth = yup_oauth2::InstalledFlowAuthenticator::builder(
        secret,
        // Interactive, not HTTPRedirect: the redirect flow ignores the
        // delegate's refusal and waits forever for a browser callback, while
        // the interactive flow propagates it as an error.
        yup_oauth2::InstalledFlowReturnMethod::Interactive,
    )
    .persist_tokens_to_disk(cache_path)
    .flow_delegate(Box::new(NoInteractionDelegate))
    .build()
    .await
    .context("failed to build authenticator")?;
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
