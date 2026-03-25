use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Duration, NaiveDate};
use clap::{Args, CommandFactory, Parser, Subcommand};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use urlencoding::encode;

const API_BASE: &str = "https://www.googleapis.com/calendar/v3";
const CALENDAR_SCOPE: &str = "https://www.googleapis.com/auth/calendar";

// --- Config file ---

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Config {
    calendar_name: Option<String>,
    no_browser: Option<bool>,
    /// Allowed property definitions: key -> list of allowed values.
    properties: Option<std::collections::HashMap<String, Vec<String>>>,
}

impl Config {
    fn load() -> Self {
        let path = config_path();
        if !path.exists() {
            return Self::default();
        }
        match std::fs::read_to_string(&path) {
            Ok(contents) => toml::from_str(&contents).unwrap_or_else(|e| {
                eprintln!("Warning: failed to parse {}: {e}", path.display());
                Self::default()
            }),
            Err(e) => {
                eprintln!("Warning: failed to read {}: {e}", path.display());
                Self::default()
            }
        }
    }

    fn no_browser(&self) -> bool {
        self.no_browser.unwrap_or(false)
    }
}

// --- Config paths (mirrors rscontacts) ---

fn config_dir() -> PathBuf {
    let mut dir = dirs::home_dir().expect("Could not determine home directory");
    dir.push(".config");
    dir.push("rscalendar");
    std::fs::create_dir_all(&dir).expect("Could not create config directory");
    dir
}

fn config_path() -> PathBuf {
    config_dir().join("config.toml")
}

fn credentials_path() -> PathBuf {
    let path = config_dir().join("credentials.json");
    if !path.exists() {
        eprintln!("Error: credentials.json not found at {}", path.display());
        eprintln!("Download OAuth2 credentials from Google Cloud Console and place them there.");
        std::process::exit(1);
    }
    path
}

fn token_cache_path() -> PathBuf {
    config_dir().join("token_cache.json")
}

// --- OAuth2 flow delegates (mirrors rscontacts) ---

struct NoInteractionDelegate;

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

struct BrowserFlowDelegate;

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

// --- CLI ---

#[derive(Debug, Parser)]
#[command(
    author,
    version,
    about = "Google Calendar CLI for listing and changing events"
)]
struct Cli {
    /// Show "(built-in)" labels on standard Google Calendar fields.
    #[arg(long, global = true, default_value_t = false)]
    show_builtin: bool,

    /// Output as JSON instead of human-readable text.
    #[arg(long, global = true, default_value_t = false)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print the default configuration file.
    Defconfig,
    /// List all calendars accessible to the authenticated user.
    ListCalendars,
    /// List upcoming events for a calendar.
    List(ListArgs),
    /// Create a new event.
    Create(UpsertArgs),
    /// Update fields on an existing event.
    Update(UpdateArgs),
    /// Delete an event.
    Delete(DeleteArgs),
    /// Manage calendars.
    Calendar {
        #[command(subcommand)]
        action: CalendarAction,
    },
    /// Manage event properties.
    Properties {
        #[command(subcommand)]
        action: PropertiesAction,
    },
    /// Interactively move events from one calendar to another.
    MoveEvents(MoveEventsArgs),
    /// Authenticate with Google via OAuth2 and cache the token.
    Auth(AuthArgs),
    /// Generate shell completions.
    Complete {
        /// Shell to generate completions for.
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[derive(Debug, Subcommand)]
enum CalendarAction {
    /// Create a new public calendar.
    Create {
        /// Name of the calendar to create.
        name: String,
    },
}

#[derive(Debug, Args)]
struct MoveEventsArgs {
    /// Source calendar name to move events from.
    #[arg(long)]
    source: String,

    /// Target calendar name to move events into.
    #[arg(long)]
    target: String,

    /// Shared extended property key to set on moved events.
    #[arg(long)]
    property_key: Option<String>,

    /// Shared extended property value to set on moved events (requires --property-key).
    #[arg(long)]
    property_value: Option<String>,

    /// Show what would be done without making changes.
    #[arg(long, default_value_t = false)]
    dry_run: bool,

    /// Move all events without prompting.
    #[arg(long, default_value_t = false)]
    all: bool,
}

#[derive(Debug, Subcommand)]
enum PropertiesAction {
    /// Add properties to events (validates against config).
    Add(PropertiesAddArgs),
    /// Check that all event properties have keys and values defined in config.
    Check(PropertiesCalendarArgs),
    /// Delete a property from events.
    Delete(PropertiesDeleteArgs),
    /// Rename a property key on events.
    Rename(PropertiesRenameArgs),
}

#[derive(Debug, Args)]
struct PropertiesAddArgs {
    /// Calendar name (default: from config).
    #[arg(long)]
    calendar_name: Option<String>,

    /// Property key to set (must be defined in config). If omitted, prompts for all missing properties.
    #[arg(long)]
    key: Option<String>,

    /// Property value to set (must be allowed for the key in config). Requires --key.
    #[arg(long)]
    value: Option<String>,

    /// Apply to all events without prompting.
    #[arg(long, default_value_t = false)]
    all: bool,
}

#[derive(Debug, Args)]
struct PropertiesCalendarArgs {
    /// Calendar name (default: from config).
    #[arg(long)]
    calendar_name: Option<String>,
}

#[derive(Debug, Args)]
struct PropertiesDeleteArgs {
    /// Calendar name (default: from config).
    #[arg(long)]
    calendar_name: Option<String>,

    /// Property key to delete.
    #[arg(long)]
    key: String,

    /// Apply to all events without prompting.
    #[arg(long, default_value_t = false)]
    all: bool,
}

#[derive(Debug, Args)]
struct PropertiesRenameArgs {
    /// Calendar name (default: from config).
    #[arg(long)]
    calendar_name: Option<String>,

    /// Current property key name.
    #[arg(long)]
    from: String,

    /// New property key name.
    #[arg(long)]
    to: String,

    /// Apply to all events without prompting.
    #[arg(long, default_value_t = false)]
    all: bool,
}

#[derive(Debug, Args)]
struct ListArgs {
    /// Calendar name (default: from config).
    #[arg(long)]
    calendar_name: Option<String>,
}

#[derive(Debug, Args)]
struct UpsertArgs {
    /// Calendar name (default: from config).
    #[arg(long)]
    calendar_name: Option<String>,

    /// Event summary/title.
    #[arg(long)]
    summary: String,

    /// Event start time. Accepts RFC3339 or YYYY-MM-DD.
    #[arg(long)]
    start: String,

    /// Event end time. Accepts RFC3339 or YYYY-MM-DD. For all-day events,
    /// the provided date is treated as inclusive and converted to Google's
    /// exclusive end-date format.
    #[arg(long)]
    end: String,

    /// Optional description/body text.
    #[arg(long)]
    description: Option<String>,

    /// Optional location string.
    #[arg(long)]
    location: Option<String>,
}

#[derive(Debug, Args)]
struct UpdateArgs {
    /// Calendar name (default: from config).
    #[arg(long)]
    calendar_name: Option<String>,

    /// Event ID to update.
    #[arg(long)]
    event_id: String,

    /// Replacement event summary/title.
    #[arg(long)]
    summary: Option<String>,

    /// Replacement start time. Accepts RFC3339 or YYYY-MM-DD.
    #[arg(long)]
    start: Option<String>,

    /// Replacement end time. Accepts RFC3339 or YYYY-MM-DD.
    #[arg(long)]
    end: Option<String>,

    /// Replacement description.
    #[arg(long)]
    description: Option<String>,

    /// Replacement location.
    #[arg(long)]
    location: Option<String>,
}

#[derive(Debug, Args)]
struct DeleteArgs {
    /// Calendar name (default: from config).
    #[arg(long)]
    calendar_name: Option<String>,

    /// Event ID to delete.
    #[arg(long)]
    event_id: String,
}

#[derive(Debug, Args)]
struct AuthArgs {
    /// Print the authorization URL instead of opening the browser.
    #[arg(long, default_value_t = false)]
    no_browser: bool,

    /// Force re-authentication by removing cached token first.
    #[arg(long, default_value_t = false)]
    force: bool,
}

// --- Auth command (mirrors rscontacts) ---

async fn cmd_auth(args: &AuthArgs, config: &Config) -> Result<()> {
    if args.force {
        let cache = token_cache_path();
        if cache.exists() {
            std::fs::remove_file(&cache)?;
            eprintln!("Removed cached token at {}", cache.display());
        }
    }

    let secret = yup_oauth2::read_application_secret(credentials_path())
        .await
        .context("failed to read credentials.json")?;

    let mut builder = yup_oauth2::InstalledFlowAuthenticator::builder(
        secret,
        yup_oauth2::InstalledFlowReturnMethod::HTTPRedirect,
    )
    .persist_tokens_to_disk(token_cache_path());

    let no_browser = args.no_browser || config.no_browser();
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
        token_cache_path().display()
    );
    Ok(())
}

// --- Google Calendar client using cached token ---

#[derive(Clone)]
struct GoogleCalendarClient {
    http: Client,
    access_token: String,
}

impl GoogleCalendarClient {
    async fn from_cache() -> Result<Self> {
        let cache_path = token_cache_path();
        if !cache_path.exists() {
            eprintln!("Error: not authenticated. Run 'rscalendar auth' first.");
            std::process::exit(1);
        }

        let secret = yup_oauth2::read_application_secret(credentials_path())
            .await
            .context("failed to read credentials.json")?;

        let auth = yup_oauth2::InstalledFlowAuthenticator::builder(
            secret,
            yup_oauth2::InstalledFlowReturnMethod::HTTPRedirect,
        )
        .persist_tokens_to_disk(cache_path)
        .flow_delegate(Box::new(NoInteractionDelegate))
        .build()
        .await
        .context("failed to build authenticator")?;

        let token = auth
            .token(&[CALENDAR_SCOPE])
            .await
            .context("failed to obtain access token; try running 'rscalendar auth' again")?;

        let access_token = token.token().context("token has no access_token field")?.to_string();

        Ok(Self {
            http: Client::new(),
            access_token,
        })
    }

    async fn create_calendar(&self, summary: &str) -> Result<Value> {
        let url = format!("{}/calendars", API_BASE);
        let response = self
            .authorized(self.http.post(&url))
            .json(&json!({ "summary": summary }))
            .send()
            .await
            .context("failed to create calendar")?;
        let response = response.error_for_status().map_err(api_error)?;
        let calendar: Value = response.json().await.context("failed to decode created calendar")?;

        // Make the calendar public by inserting an ACL rule
        let cal_id = calendar["id"].as_str().context("created calendar has no id")?;
        let acl_url = format!("{}/calendars/{}/acl", API_BASE, encode(cal_id));
        self.authorized(self.http.post(&acl_url))
            .json(&json!({
                "role": "reader",
                "scope": { "type": "default" }
            }))
            .send()
            .await
            .context("failed to set calendar ACL")?
            .error_for_status()
            .map_err(api_error)?;

        Ok(calendar)
    }

    async fn list_all_events(&self, calendar_id: &str) -> Result<Vec<CalendarEvent>> {
        let mut all_events = Vec::new();
        let mut page_token: Option<String> = None;

        loop {
            let url = format!(
                "{}/calendars/{}/events",
                API_BASE,
                encode(calendar_id)
            );

            let mut request = self
                .authorized(self.http.get(url))
                .query(&[
                    ("maxResults", "2500"),
                    ("singleEvents", "true"),
                    ("orderBy", "startTime"),
                ]);

            if let Some(token) = &page_token {
                request = request.query(&[("pageToken", token.as_str())]);
            }

            let response = request
                .send()
                .await
                .context("failed to call Google Calendar list events API")?;

            let response = response.error_for_status().map_err(api_error)?;
            let body: Value = response
                .json()
                .await
                .context("failed to decode event list response")?;

            if let Some(items) = body["items"].as_array() {
                let events: Vec<CalendarEvent> = serde_json::from_value(json!(items))
                    .context("failed to deserialize events")?;
                all_events.extend(events);
            }

            match body["nextPageToken"].as_str() {
                Some(token) => page_token = Some(token.to_string()),
                None => break,
            }
        }

        Ok(all_events)
    }

    async fn insert_event_raw(&self, calendar_id: &str, payload: &Value) -> Result<CalendarEvent> {
        let url = format!(
            "{}/calendars/{}/events",
            API_BASE,
            encode(calendar_id)
        );

        let response = self
            .authorized(self.http.post(url))
            .json(payload)
            .send()
            .await
            .context("failed to insert event")?;

        let response = response.error_for_status().map_err(api_error)?;
        response
            .json()
            .await
            .context("failed to decode inserted event")
    }

    async fn list_calendars(&self) -> Result<Vec<CalendarListEntry>> {
        let url = format!("{}/users/me/calendarList", API_BASE);

        let response = self
            .authorized(self.http.get(url))
            .send()
            .await
            .context("failed to call Google Calendar list calendars API")?;

        let response = response.error_for_status().map_err(api_error)?;
        let body: CalendarListResponse = response
            .json()
            .await
            .context("failed to decode calendar list response")?;
        Ok(body.items)
    }

    async fn create_event(&self, calendar_id: &str, args: &UpsertArgs) -> Result<CalendarEvent> {
        let payload = build_event_insert_payload(
            &args.summary,
            &args.start,
            &args.end,
            args.description.as_deref(),
            args.location.as_deref(),
        )?;
        let url = format!(
            "{}/calendars/{}/events",
            API_BASE,
            encode(calendar_id)
        );

        let response = self
            .authorized(self.http.post(url))
            .json(&payload)
            .send()
            .await
            .context("failed to call Google Calendar create event API")?;

        let response = response.error_for_status().map_err(api_error)?;
        response
            .json()
            .await
            .context("failed to decode created event response")
    }

    async fn update_event(&self, calendar_id: &str, args: &UpdateArgs) -> Result<CalendarEvent> {
        let payload = build_event_patch_payload(
            args.summary.as_deref(),
            args.start.as_deref(),
            args.end.as_deref(),
            args.description.as_deref(),
            args.location.as_deref(),
        )?;

        if payload.is_empty() {
            bail!("no fields were provided to update");
        }

        let url = format!(
            "{}/calendars/{}/events/{}",
            API_BASE,
            encode(calendar_id),
            encode(&args.event_id)
        );

        let response = self
            .authorized(self.http.patch(url))
            .json(&payload)
            .send()
            .await
            .context("failed to call Google Calendar update event API")?;

        let response = response.error_for_status().map_err(api_error)?;
        response
            .json()
            .await
            .context("failed to decode updated event response")
    }

    async fn patch_event_properties(
        &self,
        calendar_id: &str,
        event_id: &str,
        shared: &std::collections::HashMap<String, String>,
    ) -> Result<CalendarEvent> {
        let url = format!(
            "{}/calendars/{}/events/{}",
            API_BASE,
            encode(calendar_id),
            encode(event_id)
        );

        let payload = json!({
            "extendedProperties": {
                "shared": shared
            }
        });

        let response = self
            .authorized(self.http.patch(url))
            .json(&payload)
            .send()
            .await
            .context("failed to patch event properties")?;

        let response = response.error_for_status().map_err(api_error)?;
        response
            .json()
            .await
            .context("failed to decode patched event")
    }

    async fn delete_event(&self, calendar_id: &str, event_id: &str) -> Result<()> {
        let url = format!(
            "{}/calendars/{}/events/{}",
            API_BASE,
            encode(calendar_id),
            encode(event_id)
        );

        self.authorized(self.http.delete(url))
            .send()
            .await
            .context("failed to call Google Calendar delete event API")?
            .error_for_status()
            .map_err(api_error)?;

        Ok(())
    }

    fn authorized(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        request.bearer_auth(&self.access_token)
    }
}

// --- Data types ---

#[derive(Debug, Deserialize)]
struct CalendarListResponse {
    #[serde(default)]
    items: Vec<CalendarListEntry>,
}

#[derive(Debug, Deserialize)]
struct CalendarListEntry {
    id: Option<String>,
    summary: Option<String>,
    description: Option<String>,
    primary: Option<bool>,
    #[serde(rename = "accessRole")]
    access_role: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CalendarEvent {
    id: Option<String>,
    summary: Option<String>,
    description: Option<String>,
    location: Option<String>,
    status: Option<String>,
    html_link: Option<String>,
    start: Option<EventDateTime>,
    end: Option<EventDateTime>,
    #[serde(rename = "extendedProperties")]
    extended_properties: Option<ExtendedProperties>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct ExtendedProperties {
    #[serde(default)]
    shared: Option<std::collections::HashMap<String, String>>,
    #[serde(default)]
    private: Option<std::collections::HashMap<String, String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
struct EventDateTime {
    #[serde(rename = "dateTime", skip_serializing_if = "Option::is_none")]
    date_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    date: Option<String>,
}

impl EventDateTime {
    fn describe(&self) -> String {
        match (&self.date_time, &self.date) {
            (Some(date_time), None) => date_time.clone(),
            (None, Some(date)) => date.clone(),
            _ => "unknown".to_string(),
        }
    }
}

// --- Helpers ---

fn build_event_insert_payload(
    summary: &str,
    start: &str,
    end: &str,
    description: Option<&str>,
    location: Option<&str>,
) -> Result<Value> {
    let mut payload = Map::new();
    payload.insert("summary".to_string(), json!(summary));
    payload.insert(
        "start".to_string(),
        serde_json::to_value(parse_event_time(start, false)?)?,
    );
    payload.insert(
        "end".to_string(),
        serde_json::to_value(parse_event_time(end, true)?)?,
    );

    if let Some(description) = description {
        payload.insert("description".to_string(), json!(description));
    }

    if let Some(location) = location {
        payload.insert("location".to_string(), json!(location));
    }

    Ok(Value::Object(payload))
}

fn build_event_patch_payload(
    summary: Option<&str>,
    start: Option<&str>,
    end: Option<&str>,
    description: Option<&str>,
    location: Option<&str>,
) -> Result<Map<String, Value>> {
    let mut payload = Map::new();

    if let Some(summary) = summary {
        payload.insert("summary".to_string(), json!(summary));
    }

    if let Some(start) = start {
        payload.insert(
            "start".to_string(),
            serde_json::to_value(parse_event_time(start, false)?)?,
        );
    }

    if let Some(end) = end {
        payload.insert(
            "end".to_string(),
            serde_json::to_value(parse_event_time(end, true)?)?,
        );
    }

    if let Some(description) = description {
        payload.insert("description".to_string(), json!(description));
    }

    if let Some(location) = location {
        payload.insert("location".to_string(), json!(location));
    }

    Ok(payload)
}

fn parse_event_time(input: &str, end_of_all_day_event: bool) -> Result<EventDateTime> {
    if let Ok(date_time) = DateTime::parse_from_rfc3339(input) {
        return Ok(EventDateTime {
            date_time: Some(date_time.to_rfc3339()),
            date: None,
        });
    }

    let date = NaiveDate::parse_from_str(input, "%Y-%m-%d")
        .with_context(|| format!("failed to parse '{input}' as RFC3339 or YYYY-MM-DD"))?;

    let date = if end_of_all_day_event {
        date.checked_add_signed(Duration::days(1))
            .ok_or_else(|| anyhow!("date overflow while adjusting all-day event end date"))?
    } else {
        date
    };

    Ok(EventDateTime {
        date_time: None,
        date: Some(date.format("%Y-%m-%d").to_string()),
    })
}

fn resolve_calendar_id<'a>(
    calendars: &'a [CalendarListEntry],
    name: Option<&str>,
    config: &Config,
) -> Result<&'a str> {
    let name = name
        .or(config.calendar_name.as_deref())
        .context("no calendar name specified; use --calendar-name or set calendar_name in config.toml")?;
    let cal = calendars
        .iter()
        .find(|c| c.summary.as_deref() == Some(name))
        .with_context(|| format!("no calendar named '{name}' found"))?;
    cal.id
        .as_deref()
        .context("calendar has no id")
}

fn prompt_select(prompt: &str, options: &[String]) -> Result<Option<String>> {
    use std::io::{Write, BufRead};
    eprintln!("{prompt}");
    for (i, opt) in options.iter().enumerate() {
        eprintln!("  {}: {opt}", i + 1);
    }
    eprintln!("  s: skip this property");
    eprint!("  choice: ");
    std::io::stderr().flush()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    let trimmed = line.trim().to_lowercase();
    if trimmed == "s" || trimmed == "skip" {
        return Ok(None);
    }
    if let Ok(n) = trimmed.parse::<usize>() {
        if n >= 1 && n <= options.len() {
            return Ok(Some(options[n - 1].clone()));
        }
    }
    eprintln!("  invalid choice, skipping");
    Ok(None)
}

fn prompt_yes_no_quit(message: &str) -> Result<Option<bool>> {
    use std::io::{Write, BufRead};
    loop {
        eprint!("{message} [y/n/q]: ");
        std::io::stderr().flush()?;
        let mut line = String::new();
        std::io::stdin().lock().read_line(&mut line)?;
        match line.trim().to_lowercase().as_str() {
            "y" | "yes" => return Ok(Some(true)),
            "n" | "no" => return Ok(Some(false)),
            "q" | "quit" => return Ok(None),
            _ => eprintln!("  Please enter y, n, or q"),
        }
    }
}

fn api_error(error: reqwest::Error) -> anyhow::Error {
    if let Some(status) = error.status() {
        anyhow!("Google Calendar API request failed with status {status}")
    } else {
        anyhow!(error)
    }
}

fn print_event(event: &CalendarEvent, show_builtin: bool, json_output: bool) {
    if json_output {
        let mut obj = Map::new();
        if let Some(id) = &event.id {
            obj.insert("id".to_string(), json!(id));
        }
        if let Some(summary) = &event.summary {
            obj.insert("summary".to_string(), json!(summary));
        }
        if let Some(start) = &event.start {
            obj.insert("start".to_string(), json!(start.describe()));
        }
        if let Some(end) = &event.end {
            obj.insert("end".to_string(), json!(end.describe()));
        }
        if let Some(status) = &event.status {
            obj.insert("status".to_string(), json!(status));
        }
        if let Some(location) = &event.location {
            obj.insert("location".to_string(), json!(location));
        }
        if let Some(description) = &event.description {
            obj.insert("description".to_string(), json!(description));
        }
        if let Some(html_link) = &event.html_link {
            obj.insert("link".to_string(), json!(html_link));
        }
        if let Some(props) = &event.extended_properties {
            if let Some(shared) = &props.shared {
                obj.insert("properties".to_string(), json!(shared));
            }
        }
        println!("{}", serde_json::to_string(&Value::Object(obj)).unwrap());
        return;
    }

    let id = event.id.as_deref().unwrap_or("<missing-id>");
    let summary = event.summary.as_deref().unwrap_or("<untitled>");
    let start = event
        .start
        .as_ref()
        .map(EventDateTime::describe)
        .unwrap_or_else(|| "unknown".to_string());
    let end = event
        .end
        .as_ref()
        .map(EventDateTime::describe)
        .unwrap_or_else(|| "unknown".to_string());
    let bi = if show_builtin { " (built-in)" } else { "" };

    println!("{summary}");
    println!("  id: {id}{bi}");
    println!("  start: {start}{bi}");
    println!("  end: {end}{bi}");

    if let Some(status) = &event.status {
        println!("  status: {status}{bi}");
    }

    if let Some(location) = &event.location {
        println!("  location: {location}{bi}");
    }

    if let Some(description) = &event.description {
        println!("  description: {description}{bi}");
    }

    if let Some(html_link) = &event.html_link {
        println!("  link: {html_link}{bi}");
    }

    if let Some(props) = &event.extended_properties {
        if let Some(shared) = &props.shared {
            if !shared.is_empty() {
                println!("  ---");
                for (key, value) in shared {
                    println!("  {key}: {value}");
                }
            }
        }
    }

    println!();
}

// --- Main ---

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let config = Config::load();

    if let Command::Complete { shell } = &cli.command {
        clap_complete::generate(*shell, &mut Cli::command(), "rscalendar", &mut std::io::stdout());
        return Ok(());
    }

    if let Command::Defconfig = &cli.command {
        print!(
            "\
# Default calendar name
# calendar_name = \"My Calendar\"

# Don't open browser during auth (useful for headless machines)
# no_browser = false

# Allowed properties for events (used by add-properties and check-properties)
# [properties]
# company = [\"Amdocs\", \"Intel\", \"Google\"]
# course = [\"Linux Fundamentals\", \"Advanced Python\"]
"
        );
        return Ok(());
    }

    if let Command::Auth(args) = &cli.command {
        cmd_auth(args, &config).await?;
        return Ok(());
    }

    let client = GoogleCalendarClient::from_cache().await?;

    match cli.command {
        Command::ListCalendars => {
            let calendars = client.list_calendars().await?;
            if calendars.is_empty() {
                println!("No calendars found.");
            } else {
                for cal in &calendars {
                    let id = cal.id.as_deref().unwrap_or("<missing-id>");
                    let summary = cal.summary.as_deref().unwrap_or("<untitled>");
                    let primary = if cal.primary.unwrap_or(false) { " (primary)" } else { "" };
                    println!("{summary}{primary}");
                    println!("  id: {id}");
                    if let Some(role) = &cal.access_role {
                        println!("  role: {role}");
                    }
                    if let Some(desc) = &cal.description {
                        println!("  description: {desc}");
                    }
                    println!();
                }
            }
        }
        Command::List(args) => {
            let calendars = client.list_calendars().await?;
            let calendar_id = resolve_calendar_id(&calendars, args.calendar_name.as_deref(), &config)?;
            let events = client.list_all_events(calendar_id).await?;

            if events.is_empty() {
                println!("No events found.");
            } else {
                for event in &events {
                    print_event(event, cli.show_builtin, cli.json);
                }
            }
        }
        Command::Create(args) => {
            let calendars = client.list_calendars().await?;
            let calendar_id = resolve_calendar_id(&calendars, args.calendar_name.as_deref(), &config)?;
            let event = client.create_event(calendar_id, &args).await?;
            println!("Created event:");
            print_event(&event, cli.show_builtin, cli.json);
        }
        Command::Update(args) => {
            let calendars = client.list_calendars().await?;
            let calendar_id = resolve_calendar_id(&calendars, args.calendar_name.as_deref(), &config)?;
            let event = client.update_event(calendar_id, &args).await?;
            println!("Updated event:");
            print_event(&event, cli.show_builtin, cli.json);
        }
        Command::Delete(args) => {
            let calendars = client.list_calendars().await?;
            let calendar_id = resolve_calendar_id(&calendars, args.calendar_name.as_deref(), &config)?;
            client.delete_event(calendar_id, &args.event_id).await?;
            println!("Deleted event '{}'.", args.event_id);
        }
        Command::Properties { action } => match action {
            PropertiesAction::Add(args) => {
                if args.key.is_some() != args.value.is_some() {
                    bail!("--key and --value must be used together");
                }

                let properties = config.properties.as_ref()
                    .context("no [properties] section in config.toml")?;
                if properties.is_empty() {
                    bail!("no properties defined in [properties] section of config.toml");
                }

                if let (Some(key), Some(value)) = (&args.key, &args.value) {
                    let allowed = properties.get(key)
                        .with_context(|| format!("key '{key}' is not defined in [properties] in config.toml"))?;
                    if !allowed.contains(value) {
                        bail!(
                            "value '{value}' is not allowed for key '{key}'. Allowed: {}",
                            allowed.join(", ")
                        );
                    }
                }

                let calendars = client.list_calendars().await?;
                let calendar_id = resolve_calendar_id(&calendars, args.calendar_name.as_deref(), &config)?;
                let events = client.list_all_events(calendar_id).await?;

                if events.is_empty() {
                    println!("No events found.");
                    return Ok(());
                }

                let sorted_keys: Vec<&String> = {
                    let mut keys: Vec<_> = properties.keys().collect();
                    keys.sort();
                    keys
                };

                println!("Adding properties to {} event(s)\n", events.len());

                let mut updated = 0u32;
                let mut skipped = 0u32;
                for event in &events {
                    let summary = event.summary.as_deref().unwrap_or("<untitled>");
                    let start = event
                        .start
                        .as_ref()
                        .map(EventDateTime::describe)
                        .unwrap_or_else(|| "unknown".to_string());
                    let event_id = match &event.id {
                        Some(id) => id,
                        None => continue,
                    };

                    let existing: std::collections::HashMap<String, String> = event
                        .extended_properties
                        .as_ref()
                        .and_then(|p| p.shared.clone())
                        .unwrap_or_default();

                    if let (Some(key), Some(value)) = (&args.key, &args.value) {
                        let already_set = existing.get(key).is_some_and(|v| v == value);
                        if already_set {
                            skipped += 1;
                            continue;
                        }

                        if !args.all {
                            let prompt = format!("Set {key}={value} on '{summary}' ({start})?");
                            match prompt_yes_no_quit(&prompt)? {
                                Some(true) => {}
                                Some(false) => {
                                    skipped += 1;
                                    continue;
                                }
                                None => {
                                    println!("\nQuit. {updated} updated, {skipped} skipped.");
                                    return Ok(());
                                }
                            }
                        }

                        let mut new_props = existing;
                        new_props.insert(key.clone(), value.clone());
                        client.patch_event_properties(calendar_id, event_id, &new_props).await?;
                        println!("  set on: {summary} ({start})");
                        updated += 1;
                    } else {
                        eprintln!("Event: {summary} ({start})");
                        if !existing.is_empty() {
                            for (k, v) in &existing {
                                eprintln!("  existing {k}: {v}");
                            }
                        }

                        let mut new_props = existing.clone();
                        let mut changed = false;

                        for key in &sorted_keys {
                            let values = &properties[*key];
                            if existing.contains_key(*key) {
                                continue;
                            }
                            let prompt = format!("  Select {key}:");
                            if let Some(value) = prompt_select(&prompt, values)? {
                                new_props.insert((*key).clone(), value);
                                changed = true;
                            }
                        }

                        if changed {
                            client.patch_event_properties(calendar_id, event_id, &new_props).await?;
                            eprintln!("  updated\n");
                            updated += 1;
                        } else {
                            eprintln!("  no changes\n");
                            skipped += 1;
                        }
                    }
                }

                println!("Done. {updated} updated, {skipped} skipped.");
            }
            PropertiesAction::Check(args) => {
                let properties = config.properties.as_ref()
                    .context("no [properties] section in config.toml")?;
                if properties.is_empty() {
                    bail!("no properties defined in [properties] section of config.toml");
                }

                let calendars = client.list_calendars().await?;
                let calendar_id = resolve_calendar_id(&calendars, args.calendar_name.as_deref(), &config)?;
                let events = client.list_all_events(calendar_id).await?;

                if events.is_empty() {
                    println!("No events found.");
                    return Ok(());
                }

                let mut issues = 0u32;
                for event in &events {
                    let summary = event.summary.as_deref().unwrap_or("<untitled>");
                    let start = event
                        .start
                        .as_ref()
                        .map(EventDateTime::describe)
                        .unwrap_or_else(|| "unknown".to_string());

                    let shared = event
                        .extended_properties
                        .as_ref()
                        .and_then(|p| p.shared.as_ref());

                    let mut event_issues: Vec<String> = Vec::new();

                    for key in properties.keys() {
                        match shared.and_then(|s| s.get(key)) {
                            None => {
                                event_issues.push(format!("missing property '{key}'"));
                            }
                            Some(value) => {
                                let allowed = &properties[key];
                                if !allowed.contains(value) {
                                    event_issues.push(format!(
                                        "property '{key}' has value '{value}' which is not in allowed values: {}",
                                        allowed.join(", ")
                                    ));
                                }
                            }
                        }
                    }

                    if let Some(shared) = shared {
                        for key in shared.keys() {
                            if !properties.contains_key(key) {
                                event_issues.push(format!("unknown property '{key}'"));
                            }
                        }
                    }

                    if !event_issues.is_empty() {
                        println!("{summary} ({start}):");
                        for issue in &event_issues {
                            println!("  - {issue}");
                        }
                        issues += event_issues.len() as u32;
                    }
                }

                if issues == 0 {
                    println!("All {} event(s) have valid properties.", events.len());
                } else {
                    println!("\n{issues} issue(s) found across {} event(s).", events.len());
                }
            }
            PropertiesAction::Delete(args) => {
                let calendars = client.list_calendars().await?;
                let calendar_id = resolve_calendar_id(&calendars, args.calendar_name.as_deref(), &config)?;
                let events = client.list_all_events(calendar_id).await?;

                if events.is_empty() {
                    println!("No events found.");
                    return Ok(());
                }

                let mut updated = 0u32;
                let mut skipped = 0u32;
                for event in &events {
                    let summary = event.summary.as_deref().unwrap_or("<untitled>");
                    let start = event
                        .start
                        .as_ref()
                        .map(EventDateTime::describe)
                        .unwrap_or_else(|| "unknown".to_string());
                    let event_id = match &event.id {
                        Some(id) => id,
                        None => continue,
                    };

                    let existing: std::collections::HashMap<String, String> = event
                        .extended_properties
                        .as_ref()
                        .and_then(|p| p.shared.clone())
                        .unwrap_or_default();

                    if !existing.contains_key(&args.key) {
                        skipped += 1;
                        continue;
                    }

                    if !args.all {
                        let current_value = &existing[&args.key];
                        let prompt = format!("Delete {}={current_value} from '{summary}' ({start})?", args.key);
                        match prompt_yes_no_quit(&prompt)? {
                            Some(true) => {}
                            Some(false) => {
                                skipped += 1;
                                continue;
                            }
                            None => {
                                println!("\nQuit. {updated} deleted, {skipped} skipped.");
                                return Ok(());
                            }
                        }
                    }

                    let url = format!(
                        "{}/calendars/{}/events/{}",
                        API_BASE,
                        encode(calendar_id),
                        encode(event_id)
                    );
                    let mut shared = Map::new();
                    shared.insert(args.key.clone(), Value::Null);
                    let payload = json!({
                        "extendedProperties": {
                            "shared": shared
                        }
                    });
                    client.authorized(client.http.patch(url))
                        .json(&payload)
                        .send()
                        .await
                        .context("failed to delete property")?
                        .error_for_status()
                        .map_err(api_error)?;
                    println!("  deleted from: {summary} ({start})");
                    updated += 1;
                }

                println!("\nDone. {updated} deleted, {skipped} skipped.");
            }
            PropertiesAction::Rename(args) => {
                let calendars = client.list_calendars().await?;
                let calendar_id = resolve_calendar_id(&calendars, args.calendar_name.as_deref(), &config)?;
                let events = client.list_all_events(calendar_id).await?;

                if events.is_empty() {
                    println!("No events found.");
                    return Ok(());
                }

                let mut updated = 0u32;
                let mut skipped = 0u32;
                for event in &events {
                    let summary = event.summary.as_deref().unwrap_or("<untitled>");
                    let start = event
                        .start
                        .as_ref()
                        .map(EventDateTime::describe)
                        .unwrap_or_else(|| "unknown".to_string());
                    let event_id = match &event.id {
                        Some(id) => id,
                        None => continue,
                    };

                    let mut existing: std::collections::HashMap<String, String> = event
                        .extended_properties
                        .as_ref()
                        .and_then(|p| p.shared.clone())
                        .unwrap_or_default();

                    let value = match existing.remove(&args.from) {
                        Some(v) => v,
                        None => {
                            skipped += 1;
                            continue;
                        }
                    };

                    if !args.all {
                        let prompt = format!(
                            "Rename '{}'='{}' to '{}'='{}' on '{summary}' ({start})?",
                            args.from, value, args.to, value
                        );
                        match prompt_yes_no_quit(&prompt)? {
                            Some(true) => {}
                            Some(false) => {
                                skipped += 1;
                                continue;
                            }
                            None => {
                                println!("\nQuit. {updated} renamed, {skipped} skipped.");
                                return Ok(());
                            }
                        }
                    }

                    existing.insert(args.to.clone(), value);
                    client.patch_event_properties(calendar_id, event_id, &existing).await?;
                    println!("  renamed on: {summary} ({start})");
                    updated += 1;
                }

                println!("\nDone. {updated} renamed, {skipped} skipped.");
            }
        },
        Command::Calendar { action } => match action {
            CalendarAction::Create { name } => {
                let calendar = client.create_calendar(&name).await?;
                let id = calendar["id"].as_str().unwrap_or("<unknown>");
                println!("Created public calendar '{name}'");
                println!("  id: {id}");
            }
        },
        Command::MoveEvents(args) => {
            if args.property_key.is_some() != args.property_value.is_some() {
                bail!("--property-key and --property-value must be used together");
            }

            let calendars = client.list_calendars().await?;

            let source_cal = calendars
                .iter()
                .find(|c| c.summary.as_deref() == Some(&args.source))
                .context(format!("no calendar named '{}' found", args.source))?;
            let source_id = source_cal
                .id
                .as_deref()
                .context("source calendar has no id")?
                .to_string();

            let target_cal = calendars
                .iter()
                .find(|c| c.summary.as_deref() == Some(&args.target))
                .context(format!("no calendar named '{}' found", args.target))?;
            let target_id = target_cal
                .id
                .as_deref()
                .context("target calendar has no id")?
                .to_string();

            println!("Moving events from '{}' to '{}'", args.source, args.target);
            if let (Some(key), Some(value)) = (&args.property_key, &args.property_value) {
                println!("  setting property: {key}={value}");
            }
            if !args.all {
                println!("  interactive mode: y=move, n=skip, q=quit");
            }
            println!();

            let events = client.list_all_events(&source_id).await?;
            if events.is_empty() {
                println!("No events found in '{}'.", args.source);
                return Ok(());
            }

            println!("Found {} event(s)\n", events.len());

            let mut moved = 0u32;
            let mut skipped = 0u32;
            for event in &events {
                let summary = event.summary.as_deref().unwrap_or("<untitled>");
                let start = event
                    .start
                    .as_ref()
                    .map(EventDateTime::describe)
                    .unwrap_or_else(|| "unknown".to_string());
                let event_id = match &event.id {
                    Some(id) => id,
                    None => {
                        eprintln!("  skipping event with no id: {summary}");
                        skipped += 1;
                        continue;
                    }
                };

                if !args.all {
                    let prompt = format!("  Move '{summary}' ({start})?");
                    match prompt_yes_no_quit(&prompt)? {
                        Some(true) => {}
                        Some(false) => {
                            skipped += 1;
                            continue;
                        }
                        None => {
                            println!("\nQuit. {moved} event(s) moved, {skipped} skipped.");
                            return Ok(());
                        }
                    }
                }

                if args.dry_run {
                    print!("  [dry-run] would move: {summary} ({start})");
                    if let (Some(key), Some(value)) = (&args.property_key, &args.property_value) {
                        print!(" with {key}={value}");
                    }
                    println!();
                    moved += 1;
                } else {
                    let mut payload = json!({
                        "summary": summary,
                    });

                    if let (Some(key), Some(value)) = (&args.property_key, &args.property_value) {
                        let mut shared = Map::new();
                        shared.insert(key.clone(), json!(value));
                        payload["extendedProperties"] = json!({
                            "shared": shared
                        });
                    }

                    if let Some(start) = &event.start {
                        payload["start"] = serde_json::to_value(start)?;
                    }
                    if let Some(end) = &event.end {
                        payload["end"] = serde_json::to_value(end)?;
                    }
                    if let Some(desc) = &event.description {
                        payload["description"] = json!(desc);
                    }
                    if let Some(loc) = &event.location {
                        payload["location"] = json!(loc);
                    }

                    client.insert_event_raw(&target_id, &payload).await?;
                    client.delete_event(&source_id, event_id).await?;
                    print!("  moved: {summary} ({start})");
                    if let (Some(key), Some(value)) = (&args.property_key, &args.property_value) {
                        print!(" with {key}={value}");
                    }
                    println!();
                    moved += 1;
                }
            }

            if args.dry_run {
                println!("\nDry run complete. {moved} event(s) would be moved, {skipped} skipped.");
            } else {
                println!("\nDone. {moved} event(s) moved to '{}', {skipped} skipped.", args.target);
            }
        }
        Command::Auth(_) | Command::Complete { .. } | Command::Defconfig => unreachable!(),
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{build_event_patch_payload, parse_event_time};

    #[test]
    fn parses_rfc3339_time() {
        let event_time = parse_event_time("2026-03-24T12:30:00+02:00", false).unwrap();
        assert_eq!(
            event_time.date_time.as_deref(),
            Some("2026-03-24T12:30:00+02:00")
        );
        assert_eq!(event_time.date, None);
    }

    #[test]
    fn adjusts_end_date_for_all_day_event() {
        let event_time = parse_event_time("2026-03-24", true).unwrap();
        assert_eq!(event_time.date_time, None);
        assert_eq!(event_time.date.as_deref(), Some("2026-03-25"));
    }

    #[test]
    fn builds_sparse_patch_payload() {
        let payload = build_event_patch_payload(
            Some("New summary"),
            None,
            Some("2026-03-24"),
            None,
            Some("Office"),
        )
        .unwrap();

        assert_eq!(payload["summary"], "New summary");
        assert_eq!(payload["location"], "Office");
        assert_eq!(payload["end"]["date"], "2026-03-25");
        assert!(payload.get("start").is_none());
        assert!(payload.get("description").is_none());
    }
}
