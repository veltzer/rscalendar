use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;

use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use clap::{Args, CommandFactory, Parser, Subcommand};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use urlencoding::encode;

const DEFAULT_CALENDAR_ID: &str = "primary";
const DEFAULT_MAX_RESULTS: u32 = 10;
const API_BASE: &str = "https://www.googleapis.com/calendar/v3";
const CALENDAR_SCOPE: &str = "https://www.googleapis.com/auth/calendar";

// --- Config file ---

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Config {
    calendar_id: Option<String>,
    max_results: Option<u32>,
    no_browser: Option<bool>,
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

    fn calendar_id(&self) -> &str {
        self.calendar_id.as_deref().unwrap_or(DEFAULT_CALENDAR_ID)
    }

    fn max_results(&self) -> u32 {
        self.max_results.unwrap_or(DEFAULT_MAX_RESULTS)
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
    /// Migrate events from "Client - *" calendars to a target calendar.
    Migrate(MigrateArgs),
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
struct MigrateArgs {
    /// Target calendar ID to copy events into.
    #[arg(long)]
    target: String,

    /// Source calendar name prefix (default: "Client - ").
    #[arg(long, default_value = "Client - ")]
    prefix: String,

    /// Show what would be done without making changes.
    #[arg(long, default_value_t = false)]
    dry_run: bool,
}

#[derive(Debug, Args)]
struct ListArgs {
    /// Calendar ID (default: from config or "primary").
    #[arg(long)]
    calendar_id: Option<String>,

    /// Number of events to return (default: from config or 10).
    #[arg(long)]
    max_results: Option<u32>,

    /// Lower bound for event start times. Accepts RFC3339; defaults to now.
    #[arg(long)]
    time_min: Option<String>,

    /// Include deleted events.
    #[arg(long, default_value_t = false)]
    show_deleted: bool,
}

#[derive(Debug, Args)]
struct UpsertArgs {
    /// Calendar ID (default: from config or "primary").
    #[arg(long)]
    calendar_id: Option<String>,

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
    /// Calendar ID (default: from config or "primary").
    #[arg(long)]
    calendar_id: Option<String>,

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
    /// Calendar ID (default: from config or "primary").
    #[arg(long)]
    calendar_id: Option<String>,

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

    async fn list_events(
        &self,
        calendar_id: &str,
        max_results: u32,
        time_min: Option<&str>,
        show_deleted: bool,
    ) -> Result<Vec<CalendarEvent>> {
        let time_min = match time_min {
            Some(value) => parse_rfc3339(value)?.to_rfc3339(),
            None => Utc::now().to_rfc3339(),
        };

        let url = format!(
            "{}/calendars/{}/events",
            API_BASE,
            encode(calendar_id)
        );

        let response = self
            .authorized(self.http.get(url))
            .query(&[
                ("maxResults", max_results.to_string()),
                ("orderBy", "startTime".to_string()),
                ("showDeleted", show_deleted.to_string()),
                ("singleEvents", "true".to_string()),
                ("timeMin", time_min),
            ])
            .send()
            .await
            .context("failed to call Google Calendar list events API")?;

        let response = response.error_for_status().map_err(api_error)?;
        let body: EventListResponse = response
            .json()
            .await
            .context("failed to decode Google Calendar list response")?;
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
struct EventListResponse {
    #[serde(default)]
    items: Vec<CalendarEvent>,
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

fn parse_rfc3339(input: &str) -> Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(input)
        .with_context(|| format!("failed to parse '{input}' as RFC3339"))?
        .with_timezone(&Utc))
}

fn api_error(error: reqwest::Error) -> anyhow::Error {
    if let Some(status) = error.status() {
        anyhow!("Google Calendar API request failed with status {status}")
    } else {
        anyhow!(error)
    }
}

fn print_event(event: &CalendarEvent) {
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

    println!("{summary}");
    println!("  id: {id}");
    println!("  start: {start}");
    println!("  end: {end}");

    if let Some(status) = &event.status {
        println!("  status: {status}");
    }

    if let Some(location) = &event.location {
        println!("  location: {location}");
    }

    if let Some(description) = &event.description {
        println!("  description: {description}");
    }

    if let Some(html_link) = &event.html_link {
        println!("  link: {html_link}");
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
# Default calendar ID
# calendar_id = \"primary\"

# Default number of events to show in list
# max_results = 10

# Don't open browser during auth (useful for headless machines)
# no_browser = false
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
            let calendar_id = args.calendar_id.as_deref().unwrap_or(config.calendar_id());
            let max_results = args.max_results.unwrap_or(config.max_results());
            let events = client
                .list_events(calendar_id, max_results, args.time_min.as_deref(), args.show_deleted)
                .await?;

            if events.is_empty() {
                println!("No events found.");
            } else {
                for event in &events {
                    print_event(event);
                }
            }
        }
        Command::Create(args) => {
            let calendar_id = args.calendar_id.as_deref().unwrap_or(config.calendar_id());
            let event = client.create_event(calendar_id, &args).await?;
            println!("Created event:");
            print_event(&event);
        }
        Command::Update(args) => {
            let calendar_id = args.calendar_id.as_deref().unwrap_or(config.calendar_id());
            let event = client.update_event(calendar_id, &args).await?;
            println!("Updated event:");
            print_event(&event);
        }
        Command::Delete(args) => {
            let calendar_id = args.calendar_id.as_deref().unwrap_or(config.calendar_id());
            client.delete_event(calendar_id, &args.event_id).await?;
            println!(
                "Deleted event '{}' from calendar '{}'.",
                args.event_id, calendar_id
            );
        }
        Command::Calendar { action } => match action {
            CalendarAction::Create { name } => {
                let calendar = client.create_calendar(&name).await?;
                let id = calendar["id"].as_str().unwrap_or("<unknown>");
                println!("Created public calendar '{name}'");
                println!("  id: {id}");
            }
        },
        Command::Migrate(args) => {
            let calendars = client.list_calendars().await?;
            let matching: Vec<_> = calendars
                .iter()
                .filter(|c| {
                    c.summary
                        .as_deref()
                        .is_some_and(|s| s.starts_with(&args.prefix))
                })
                .collect();

            if matching.is_empty() {
                println!("No calendars matching prefix '{}' found.", args.prefix);
                return Ok(());
            }

            println!(
                "Found {} calendar(s) matching '{}':",
                matching.len(),
                args.prefix
            );
            for cal in &matching {
                println!("  - {}", cal.summary.as_deref().unwrap_or("<untitled>"));
            }
            println!();

            let mut total = 0u32;
            for cal in &matching {
                let cal_id = match &cal.id {
                    Some(id) => id,
                    None => continue,
                };
                let cal_name = cal.summary.as_deref().unwrap_or("<untitled>");
                let client_name = cal_name.strip_prefix(&args.prefix).unwrap_or(cal_name);

                let events = client.list_all_events(cal_id).await?;
                if events.is_empty() {
                    println!("{cal_name}: no events");
                    continue;
                }

                println!("{cal_name}: {} event(s)", events.len());

                for event in &events {
                    let summary = event.summary.as_deref().unwrap_or("<untitled>");
                    let start = event
                        .start
                        .as_ref()
                        .map(EventDateTime::describe)
                        .unwrap_or_else(|| "unknown".to_string());

                    if args.dry_run {
                        println!("  [dry-run] would copy: {summary} ({start}) with client={client_name}");
                    } else {
                        let mut payload = json!({
                            "summary": summary,
                            "extendedProperties": {
                                "shared": {
                                    "client": client_name
                                }
                            }
                        });

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

                        client.insert_event_raw(&args.target, &payload).await?;
                        println!("  copied: {summary} ({start}) with client={client_name}");
                    }
                    total += 1;
                }
            }

            if args.dry_run {
                println!("\nDry run complete. {total} event(s) would be copied.");
            } else {
                println!("\nDone. {total} event(s) copied to '{}'.", args.target);
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
