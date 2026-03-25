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
            Err("Not authenticated. Run 'rscalendar auth' first.".to_string())
        })
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
