use std::env;

use anyhow::{Context, Result, anyhow, bail};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use clap::{Args, CommandFactory, Parser, Subcommand};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use urlencoding::encode;

const ACCESS_TOKEN_ENV: &str = "GOOGLE_CALENDAR_ACCESS_TOKEN";
const CLIENT_ID_ENV: &str = "GOOGLE_CLIENT_ID";
const CLIENT_SECRET_ENV: &str = "GOOGLE_CLIENT_SECRET";
const DEFAULT_CALENDAR_ID: &str = "primary";
const API_BASE: &str = "https://www.googleapis.com/calendar/v3";

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
    /// List upcoming events for a calendar.
    List(ListArgs),
    /// Create a new event.
    Create(UpsertArgs),
    /// Update fields on an existing event.
    Update(UpdateArgs),
    /// Delete an event.
    Delete(DeleteArgs),
    /// Authenticate with Google via OAuth2 device flow and print the access token.
    Auth(AuthArgs),
    /// Generate shell completions.
    Complete {
        /// Shell to generate completions for.
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[derive(Debug, Args)]
struct ListArgs {
    /// Calendar ID. Defaults to the authenticated user's primary calendar.
    #[arg(long, default_value = DEFAULT_CALENDAR_ID)]
    calendar_id: String,

    /// Number of events to return.
    #[arg(long, default_value_t = 10)]
    max_results: u32,

    /// Lower bound for event start times. Accepts RFC3339; defaults to now.
    #[arg(long)]
    time_min: Option<String>,

    /// Include deleted events.
    #[arg(long, default_value_t = false)]
    show_deleted: bool,
}

#[derive(Debug, Args)]
struct UpsertArgs {
    /// Calendar ID. Defaults to the authenticated user's primary calendar.
    #[arg(long, default_value = DEFAULT_CALENDAR_ID)]
    calendar_id: String,

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
    /// Calendar ID. Defaults to the authenticated user's primary calendar.
    #[arg(long, default_value = DEFAULT_CALENDAR_ID)]
    calendar_id: String,

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
    /// Calendar ID. Defaults to the authenticated user's primary calendar.
    #[arg(long, default_value = DEFAULT_CALENDAR_ID)]
    calendar_id: String,

    /// Event ID to delete.
    #[arg(long)]
    event_id: String,
}

#[derive(Debug, Args)]
struct AuthArgs {
    /// Google OAuth2 client ID. Falls back to GOOGLE_CLIENT_ID env var.
    #[arg(long, env = CLIENT_ID_ENV)]
    client_id: String,

    /// Google OAuth2 client secret. Falls back to GOOGLE_CLIENT_SECRET env var.
    #[arg(long, env = CLIENT_SECRET_ENV)]
    client_secret: String,

    /// Open the verification URL in the default browser automatically.
    #[arg(long, default_value_t = false)]
    open_browser: bool,
}

#[derive(Debug, Deserialize)]
struct DeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_url: String,
    interval: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    error: Option<String>,
}

const DEVICE_CODE_URL: &str = "https://oauth2.googleapis.com/device/code";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const CALENDAR_SCOPE: &str = "https://www.googleapis.com/auth/calendar";

async fn device_auth_flow(args: &AuthArgs) -> Result<()> {
    let http = Client::new();

    // Step 1: Request device & user codes.
    let response: DeviceCodeResponse = http
        .post(DEVICE_CODE_URL)
        .form(&[
            ("client_id", args.client_id.as_str()),
            ("scope", CALENDAR_SCOPE),
        ])
        .send()
        .await
        .context("failed to request device code")?
        .error_for_status()
        .map_err(api_error)?
        .json()
        .await
        .context("failed to decode device code response")?;

    eprintln!("To authorize rscalendar, visit:");
    eprintln!();
    eprintln!("  {}", response.verification_url);
    eprintln!();
    eprintln!("and enter code: {}", response.user_code);
    eprintln!();

    if args.open_browser {
        let _ = open::that(&response.verification_url);
    }

    // Step 2: Poll for the token.
    let interval = std::time::Duration::from_secs(response.interval.unwrap_or(5));

    loop {
        tokio::time::sleep(interval).await;

        let token_resp: TokenResponse = http
            .post(TOKEN_URL)
            .form(&[
                ("client_id", args.client_id.as_str()),
                ("client_secret", args.client_secret.as_str()),
                ("device_code", response.device_code.as_str()),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ])
            .send()
            .await
            .context("failed to poll for token")?
            .json()
            .await
            .context("failed to decode token response")?;

        if let Some(access_token) = token_resp.access_token {
            eprintln!("Authentication successful!");
            eprintln!();
            if let Some(refresh_token) = token_resp.refresh_token {
                eprintln!("Refresh token (save this for long-lived access):");
                println!("{refresh_token}");
                eprintln!();
                eprintln!("Access token:");
            }
            println!("{access_token}");
            eprintln!();
            eprintln!(
                "Export it with:  export {ACCESS_TOKEN_ENV}=<access_token>"
            );
            return Ok(());
        }

        match token_resp.error.as_deref() {
            Some("authorization_pending") => continue,
            Some("slow_down") => {
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                continue;
            }
            Some(err) => bail!("authentication failed: {err}"),
            None => bail!("unexpected empty token response"),
        }
    }
}

#[derive(Clone)]
struct GoogleCalendarClient {
    http: Client,
    access_token: String,
}

impl GoogleCalendarClient {
    fn from_env() -> Result<Self> {
        let access_token = env::var(ACCESS_TOKEN_ENV).with_context(|| {
            format!(
                "missing required environment variable {ACCESS_TOKEN_ENV}; set it to a Google OAuth access token"
            )
        })?;

        Ok(Self {
            http: Client::new(),
            access_token,
        })
    }

    async fn list_events(&self, args: &ListArgs) -> Result<Vec<CalendarEvent>> {
        let time_min = match &args.time_min {
            Some(value) => parse_rfc3339(value)?.to_rfc3339(),
            None => Utc::now().to_rfc3339(),
        };

        let url = format!(
            "{}/calendars/{}/events",
            API_BASE,
            encode(&args.calendar_id)
        );

        let response = self
            .authorized(self.http.get(url))
            .query(&[
                ("maxResults", args.max_results.to_string()),
                ("orderBy", "startTime".to_string()),
                ("showDeleted", args.show_deleted.to_string()),
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

    async fn create_event(&self, args: &UpsertArgs) -> Result<CalendarEvent> {
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
            encode(&args.calendar_id)
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

    async fn update_event(&self, args: &UpdateArgs) -> Result<CalendarEvent> {
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
            encode(&args.calendar_id),
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

    async fn delete_event(&self, args: &DeleteArgs) -> Result<()> {
        let url = format!(
            "{}/calendars/{}/events/{}",
            API_BASE,
            encode(&args.calendar_id),
            encode(&args.event_id)
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

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    if let Command::Complete { shell } = &cli.command {
        clap_complete::generate(*shell, &mut Cli::command(), "rscalendar", &mut std::io::stdout());
        return Ok(());
    }

    if let Command::Auth(args) = &cli.command {
        device_auth_flow(args).await?;
        return Ok(());
    }

    let client = GoogleCalendarClient::from_env()?;

    match cli.command {
        Command::List(args) => {
            let events = client.list_events(&args).await?;

            if events.is_empty() {
                println!("No events found.");
            } else {
                for event in &events {
                    print_event(event);
                }
            }
        }
        Command::Create(args) => {
            let event = client.create_event(&args).await?;
            println!("Created event:");
            print_event(&event);
        }
        Command::Update(args) => {
            let event = client.update_event(&args).await?;
            println!("Updated event:");
            print_event(&event);
        }
        Command::Delete(args) => {
            client.delete_event(&args).await?;
            println!(
                "Deleted event '{}' from calendar '{}'.",
                args.event_id, args.calendar_id
            );
        }
        Command::Auth(_) | Command::Complete { .. } => unreachable!(),
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
