//! OAuth credentials and the token cache in pass(1), with the file fallback.
//!
//! The OAuth client JSON and the token cache are read from pass entries when
//! they exist. When the `pass` binary is missing or an entry does not exist
//! the tool behaves exactly as it always did and uses the files under the
//! config directory. Secret values are never logged; messages name the entry
//! only.
//!
//! The pass store is a git-tracked directory, so the token entry is not
//! rewritten when an access token is refreshed (hourly). Refreshed access
//! tokens live in memory; pass is written only when the stable material
//! changes: a new authorization or a changed refresh token.

use std::ffi::OsString;
use std::io::{Error, ErrorKind, Result, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use yup_oauth2::ApplicationSecret;
use yup_oauth2::storage::{TokenInfo, TokenStorage, TokenStorageError};

/// The pass entries holding the OAuth client JSON and the token cache.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entries {
    pub oauth_client: String,
    pub token: String,
}

impl Entries {
    /// The built-in entries, with each one overridable from `[pass]` in config.toml.
    #[must_use]
    pub fn new(default_oauth_client: &str, default_token: &str, overrides: &PassConfig) -> Self {
        Self {
            oauth_client: overrides
                .oauth_client
                .clone()
                .unwrap_or_else(|| default_oauth_client.to_string()),
            token: overrides
                .token
                .clone()
                .unwrap_or_else(|| default_token.to_string()),
        }
    }
}

/// The `[pass]` table of config.toml.
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(default)]
pub struct PassConfig {
    pub oauth_client: Option<String>,
    pub token: Option<String>,
}

/// Runs the `pass` binary: arguments are passed directly, never through a shell.
#[derive(Debug, Clone)]
pub struct Pass {
    program: OsString,
}

impl Default for Pass {
    fn default() -> Self {
        Self::new()
    }
}

impl Pass {
    #[must_use]
    pub fn new() -> Self {
        Self::with_program("pass")
    }

    #[must_use]
    pub fn with_program(program: impl Into<OsString>) -> Self {
        Self {
            program: program.into(),
        }
    }

    /// The content of an entry, or `None` when `pass` is missing, the entry
    /// does not exist or cannot be decrypted.
    #[must_use]
    pub fn show(&self, entry: &str) -> Option<String> {
        let output = Command::new(&self.program)
            .arg("show")
            .arg(entry)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        String::from_utf8(output.stdout).ok()
    }

    /// Create or replace an entry; the content goes in on stdin.
    ///
    /// # Errors
    ///
    /// Returns an error if `pass` cannot be run or exits unsuccessfully.
    pub fn insert(&self, entry: &str, content: &str) -> Result<()> {
        let mut child = Command::new(&self.program)
            .args(["insert", "-m", "-f"])
            .arg(entry)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| {
                Error::new(
                    e.kind(),
                    format!("failed to run pass insert for '{entry}': {e}"),
                )
            })?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(content.as_bytes())?;
        }
        let status = child.wait()?;
        if status.success() {
            Ok(())
        } else {
            Err(Error::other(format!(
                "pass insert for '{entry}' failed ({status})"
            )))
        }
    }

    /// Remove an entry.
    ///
    /// # Errors
    ///
    /// Returns an error if `pass` cannot be run or exits unsuccessfully.
    pub fn remove(&self, entry: &str) -> Result<()> {
        let status = Command::new(&self.program)
            .args(["rm", "-f"])
            .arg(entry)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|e| {
                Error::new(
                    e.kind(),
                    format!("failed to run pass rm for '{entry}': {e}"),
                )
            })?;
        if status.success() {
            Ok(())
        } else {
            Err(Error::other(format!(
                "pass rm for '{entry}' failed ({status})"
            )))
        }
    }
}

/// Where the token cache lives.
#[derive(Debug)]
pub enum TokenBackend {
    /// A pass entry. `initial` is its content when it exists.
    Pass {
        pass: Pass,
        entry: String,
        initial: Option<String>,
    },
    /// The plaintext `token_cache.json` file.
    File(PathBuf),
}

impl TokenBackend {
    /// Whether a token has been stored yet.
    #[must_use]
    pub fn exists(&self) -> bool {
        match self {
            Self::Pass { initial, .. } => initial.is_some(),
            Self::File(path) => path.exists(),
        }
    }

    /// Human readable location, for messages. Names the entry, never the content.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Pass { entry, .. } => format!("pass entry '{entry}'"),
            Self::File(path) => path.display().to_string(),
        }
    }

    /// Forget the stored token (for `auth --force`). Returns whether one existed.
    ///
    /// # Errors
    ///
    /// Returns an error if the file or the pass entry cannot be removed.
    pub fn remove(&mut self) -> Result<bool> {
        match self {
            Self::Pass {
                pass,
                entry,
                initial,
            } => {
                let existed = initial.take().is_some();
                if existed {
                    pass.remove(entry)?;
                }
                Ok(existed)
            }
            Self::File(path) => {
                if path.exists() {
                    std::fs::remove_file(path)?;
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
        }
    }

    /// A token storage for the pass backend, `None` for the file backend
    /// (which uses yup-oauth2's own file persistence).
    #[must_use]
    pub fn pass_storage(&self) -> Option<PassTokenStorage> {
        match self {
            Self::Pass {
                pass,
                entry,
                initial,
            } => Some(PassTokenStorage::new(
                pass.clone(),
                entry.clone(),
                initial.as_deref(),
            )),
            Self::File(_) => None,
        }
    }
}

/// Read the OAuth client from pass, falling back to the file.
///
/// Returns the client and whether it came from pass. `file` is only asked for
/// when pass does not have the entry.
///
/// # Errors
///
/// Returns an error if the content (from pass or from the file) is not a
/// valid OAuth client JSON, or the file cannot be read.
pub async fn load_secret(
    pass: &Pass,
    entries: &Entries,
    file: impl FnOnce() -> Result<PathBuf>,
) -> Result<(ApplicationSecret, bool)> {
    if let Some(content) = pass.show(&entries.oauth_client) {
        let secret = yup_oauth2::parse_application_secret(content).map_err(|e| {
            Error::new(
                ErrorKind::InvalidData,
                format!(
                    "pass entry '{}' is not an OAuth client JSON: {e}",
                    entries.oauth_client
                ),
            )
        })?;
        return Ok((secret, true));
    }
    let path = file()?;
    let secret = yup_oauth2::read_application_secret(&path).await?;
    Ok((secret, false))
}

/// Choose where the token cache lives.
///
/// Pass is used when the token entry exists, or when the OAuth client came
/// from pass (so a first `auth` or `auth --force` writes to pass and not to a
/// plaintext file). Otherwise the file is used.
#[must_use]
pub fn choose_token_backend(
    pass: &Pass,
    entries: &Entries,
    secret_from_pass: bool,
    file: &Path,
) -> TokenBackend {
    let initial = pass.show(&entries.token);
    if initial.is_some() || secret_from_pass {
        TokenBackend::Pass {
            pass: pass.clone(),
            entry: entries.token.clone(),
            initial,
        }
    } else {
        TokenBackend::File(file.to_path_buf())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredToken {
    scopes: Vec<String>,
    token: TokenInfo,
}

/// A yup-oauth2 token storage backed by one pass entry.
///
/// The content has the same JSON layout as yup-oauth2's own token cache file.
/// Tokens are kept in memory; pass is written only when a scope set gets its
/// first token or its refresh token changes.
pub struct PassTokenStorage {
    pass: Pass,
    entry: String,
    tokens: Mutex<Vec<StoredToken>>,
}

impl PassTokenStorage {
    /// Unparsable initial content is treated as an empty cache.
    #[must_use]
    pub fn new(pass: Pass, entry: String, initial: Option<&str>) -> Self {
        let tokens = initial
            .and_then(|content| serde_json::from_str(content).ok())
            .unwrap_or_default();
        Self {
            pass,
            entry,
            tokens: Mutex::new(tokens),
        }
    }
}

fn covers(stored: &[String], wanted: &[&str]) -> bool {
    wanted.iter().all(|w| stored.iter().any(|s| s == w))
}

impl PassTokenStorage {
    /// Record a token in memory. Returns the JSON to write to pass when the
    /// stable material changed (a first token for the scopes or a changed
    /// refresh token), `None` for a plain access token refresh.
    fn record(
        &self,
        scopes: &[&str],
        mut token: TokenInfo,
    ) -> std::result::Result<Option<String>, TokenStorageError> {
        let mut tokens = self
            .tokens
            .lock()
            .map_err(|_| TokenStorageError::Other("token cache lock poisoned".into()))?;
        let scope_list: Vec<String> = scopes.iter().map(|s| (*s).to_string()).collect();
        let index = tokens.iter().position(|t| t.scopes == scope_list);
        let previous_refresh = index.and_then(|i| tokens[i].token.refresh_token.clone());
        if token.refresh_token.is_none() {
            token.refresh_token.clone_from(&previous_refresh);
        }
        let stable_change = index.is_none() || token.refresh_token != previous_refresh;
        let stored = StoredToken {
            scopes: scope_list,
            token,
        };
        match index {
            Some(i) => tokens[i] = stored,
            None => tokens.push(stored),
        }
        if !stable_change {
            return Ok(None);
        }
        serde_json::to_string(&*tokens)
            .map(Some)
            .map_err(|e| TokenStorageError::Other(format!("cannot serialize token: {e}").into()))
    }
}

#[async_trait::async_trait]
impl TokenStorage for PassTokenStorage {
    async fn set(
        &self,
        scopes: &[&str],
        token: TokenInfo,
    ) -> std::result::Result<(), TokenStorageError> {
        if let Some(json) = self.record(scopes, token)? {
            self.pass.insert(&self.entry, &json)?;
        }
        Ok(())
    }

    async fn get(&self, scopes: &[&str]) -> Option<TokenInfo> {
        let tokens = self.tokens.lock().ok()?;
        tokens
            .iter()
            .find(|t| covers(&t.scopes, scopes))
            .map(|t| t.token.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fake `pass` keeping entries as files in a directory.
    struct FakePass {
        dir: PathBuf,
    }

    impl FakePass {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(dir.join("store")).unwrap();
            let script = format!(
                "#!/bin/sh\nS='{}/store'\ncase \"$1\" in\n\
                 show) cat \"$S/$2\" 2>/dev/null || exit 1 ;;\n\
                 insert) cat > \"$S/$4\"; echo x >> \"{}/writes\" ;;\n\
                 rm) rm -f \"$S/$3\" ;;\n\
                 *) exit 2 ;;\nesac\n",
                dir.display(),
                dir.display()
            );
            std::fs::write(dir.join("pass"), script).unwrap();
            let status = Command::new("chmod")
                .arg("+x")
                .arg(dir.join("pass"))
                .status()
                .unwrap();
            assert!(status.success());
            Self { dir }
        }

        fn pass(&self) -> Pass {
            Pass::with_program(self.dir.join("pass"))
        }

        fn put(&self, entry: &str, content: &str) {
            std::fs::write(self.dir.join("store").join(entry), content).unwrap();
        }

        fn has(&self, entry: &str) -> bool {
            self.dir.join("store").join(entry).exists()
        }

        fn writes(&self) -> usize {
            std::fs::read_to_string(self.dir.join("writes")).map_or(0, |s| s.lines().count())
        }
    }

    impl Drop for FakePass {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn entries() -> Entries {
        Entries::new("a-oauth", "a-token", &PassConfig::default())
    }

    fn token(access: &str, refresh: &str) -> TokenInfo {
        TokenInfo {
            access_token: Some(access.to_string()),
            refresh_token: Some(refresh.to_string()),
            expires_at: None,
            id_token: None,
        }
    }

    const CLIENT: &str = r#"{"installed":{"client_id":"id","client_secret":"s",
        "auth_uri":"https://a/auth","token_uri":"https://a/token","redirect_uris":["http://localhost"]}}"#;

    #[test]
    fn config_overrides_default_entries() {
        let overrides = PassConfig {
            oauth_client: Some("x/client".into()),
            token: None,
        };
        let e = Entries::new("d/client", "d/token", &overrides);
        assert_eq!(e.oauth_client, "x/client");
        assert_eq!(e.token, "d/token");
    }

    #[test]
    fn missing_pass_binary_means_no_entry() {
        let pass = Pass::with_program("/nonexistent/pass-binary");
        assert!(pass.show("anything").is_none());
        assert!(pass.insert("anything", "x").is_err());
    }

    #[test]
    fn missing_pass_chooses_file_backend() {
        let pass = Pass::with_program("/nonexistent/pass-binary");
        let backend =
            choose_token_backend(&pass, &entries(), false, Path::new("/x/token_cache.json"));
        assert!(matches!(backend, TokenBackend::File(_)));
        assert!(!backend.exists() || Path::new("/x/token_cache.json").exists());
    }

    #[tokio::test]
    async fn secret_comes_from_pass_when_present() {
        let fake = FakePass::new("pass-secret");
        fake.put("a-oauth", CLIENT);
        let (secret, from_pass) = load_secret(&fake.pass(), &entries(), || {
            Err(Error::other("file must not be asked for"))
        })
        .await
        .unwrap();
        assert!(from_pass);
        assert_eq!(secret.client_id, "id");
    }

    #[tokio::test]
    async fn secret_falls_back_to_file() {
        let fake = FakePass::new("pass-secret-file");
        let file = fake.dir.join("credentials.json");
        std::fs::write(&file, CLIENT).unwrap();
        let (secret, from_pass) = load_secret(&fake.pass(), &entries(), || Ok(file.clone()))
            .await
            .unwrap();
        assert!(!from_pass);
        assert_eq!(secret.client_id, "id");
    }

    #[test]
    fn pass_credentials_imply_pass_token_even_when_absent() {
        let fake = FakePass::new("pass-choose");
        let backend = choose_token_backend(&fake.pass(), &entries(), true, Path::new("/x/t.json"));
        assert!(matches!(backend, TokenBackend::Pass { .. }));
        assert!(!backend.exists());
        fake.put("a-token", "[]");
        let backend = choose_token_backend(&fake.pass(), &entries(), false, Path::new("/x/t.json"));
        assert!(matches!(backend, TokenBackend::Pass { .. }));
        assert!(backend.exists());
    }

    #[tokio::test]
    async fn writes_only_when_stable_material_changes() {
        let fake = FakePass::new("pass-writes");
        let storage = PassTokenStorage::new(fake.pass(), "a-token".into(), None);
        let scopes = ["https://scope/a"];
        storage.set(&scopes, token("one", "r1")).await.unwrap();
        assert_eq!(fake.writes(), 1);
        // an hourly access token refresh: same refresh token, no write
        storage.set(&scopes, token("two", "r1")).await.unwrap();
        // a refresh response without a refresh token keeps the old one, no write
        let mut bare = token("three", "r1");
        bare.refresh_token = None;
        storage.set(&scopes, bare).await.unwrap();
        assert_eq!(fake.writes(), 1);
        let got = storage.get(&scopes).await.unwrap();
        assert_eq!(got.access_token.as_deref(), Some("three"));
        assert_eq!(got.refresh_token.as_deref(), Some("r1"));
        // a changed refresh token is written
        storage.set(&scopes, token("four", "r2")).await.unwrap();
        assert_eq!(fake.writes(), 2);
        // and what was written is read back by a fresh storage
        let content = fake.pass().show("a-token").unwrap();
        let again = PassTokenStorage::new(fake.pass(), "a-token".into(), Some(&content));
        let got = again.get(&scopes).await.unwrap();
        assert_eq!(got.refresh_token.as_deref(), Some("r2"));
    }

    #[test]
    fn remove_deletes_the_pass_entry() {
        let fake = FakePass::new("pass-remove");
        fake.put("a-token", "[]");
        let mut backend =
            choose_token_backend(&fake.pass(), &entries(), true, Path::new("/x/t.json"));
        assert!(backend.remove().unwrap());
        assert!(!fake.has("a-token"));
        assert!(!backend.exists());
    }
}
