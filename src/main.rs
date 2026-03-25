mod auth;
mod client;
mod config;
mod helpers;
mod models;

use std::collections::BTreeMap;

use anyhow::{Context, Result, bail};
use chrono::NaiveDate;
use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use serde_json::json;

use client::{GoogleCalendarClient, resolve_calendar_id};
use config::Config;
use helpers::{build_event_patch_payload, parse_event_time, prompt_select, prompt_yes_no_quit};
use models::{print_calendar, print_event};

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

    /// Suppress informational output; only show data and errors.
    #[arg(long, global = true, default_value_t = false)]
    quiet: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Print version and build information.
    Version,
    /// Print the default configuration file.
    Defconfig,
    /// List all calendars accessible to the authenticated user.
    ListCalendars,
    /// List all events for a calendar.
    List(ListArgs),
    /// Manage individual events.
    Event {
        #[command(subcommand)]
        action: EventAction,
    },
    /// Manage calendars.
    Calendar {
        #[command(subcommand)]
        action: CalendarAction,
    },
    /// Check that events have all required properties based on their type.
    Check(CheckArgs),
    /// Manage event properties.
    Properties {
        #[command(subcommand)]
        action: PropertiesAction,
    },
    /// Print statistics about events (counts by type, client, month).
    Stats(CalendarNameArgs),
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
    /// Delete a calendar.
    Delete {
        /// Name of the calendar to delete.
        name: String,
    },
    /// Rename a calendar.
    Rename {
        /// Current name of the calendar.
        name: String,
        /// New name for the calendar.
        new_name: String,
    },
    /// Delete all events from a calendar.
    Clear(CalendarClearArgs),
    /// Copy all events from one calendar to another.
    Copy(CalendarCopyArgs),
}

#[derive(Debug, Subcommand)]
enum EventAction {
    /// Create a new event.
    Create(UpsertArgs),
    /// Update fields on an existing event.
    Update(UpdateArgs),
    /// Delete an event.
    Delete(DeleteArgs),
}

#[derive(Debug, Subcommand)]
enum PropertiesAction {
    /// Add properties to events (validates against config).
    Add(PropertiesAddArgs),
    /// Check that all event properties have keys and values defined in config.
    Check(CalendarNameArgs),
    /// Delete a property from events.
    Delete(PropertiesDeleteArgs),
    /// Rename a property key on events.
    Rename(PropertiesRenameArgs),
    /// Interactively edit properties on each event via TUI menus.
    Edit(CalendarNameArgs),
    /// Change a property value on events (from one value to another).
    SetValue(PropertiesSetValueArgs),
}

// --- Shared args ---

#[derive(Debug, Args)]
struct CalendarNameArgs {
    /// Calendar name (default: from config).
    #[arg(long)]
    calendar_name: Option<String>,
}

#[derive(Debug, Args)]
struct CheckArgs {
    /// Calendar name (default: from config).
    #[arg(long)]
    calendar_name: Option<String>,
    /// Interactively fix events that have issues by prompting to set missing properties.
    #[arg(long, default_value_t = false)]
    fix: bool,
}

#[derive(Debug, Args)]
struct MoveEventsArgs {
    /// Source calendar name to move events from.
    #[arg(long)]
    source: String,
    /// Target calendar name to move events into.
    #[arg(long)]
    target: String,
    /// Show what would be done without making changes.
    #[arg(long, default_value_t = false)]
    dry_run: bool,
    /// Move all events without prompting.
    #[arg(long, default_value_t = false)]
    all: bool,
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
struct PropertiesSetValueArgs {
    /// Calendar name (default: from config).
    #[arg(long)]
    calendar_name: Option<String>,
    /// Property key to change.
    #[arg(long)]
    key: String,
    /// Current value to match.
    #[arg(long)]
    from: String,
    /// New value to set.
    #[arg(long)]
    to: String,
    /// Apply to all matching events without prompting.
    #[arg(long, default_value_t = false)]
    all: bool,
}

#[derive(Debug, Args)]
struct CalendarClearArgs {
    /// Calendar name (default: from config).
    #[arg(long)]
    calendar_name: Option<String>,
    /// Delete all events without prompting.
    #[arg(long, default_value_t = false)]
    all: bool,
}

#[derive(Debug, Args)]
struct CalendarCopyArgs {
    /// Source calendar name to copy events from.
    #[arg(long)]
    source: String,
    /// Target calendar name to copy events into.
    #[arg(long)]
    target: String,
    /// Show what would be done without making changes.
    #[arg(long, default_value_t = false)]
    dry_run: bool,
}

#[derive(Debug, Clone, ValueEnum)]
enum ListFormat {
    /// Default human-readable output.
    Default,
    /// Aligned table with columns: summary, start, end, type, client.
    Table,
}

#[derive(Debug, Args)]
struct ListArgs {
    /// Calendar name (default: from config).
    #[arg(long)]
    calendar_name: Option<String>,
    /// Only show events starting after this date (RFC3339 or YYYY-MM-DD).
    #[arg(long)]
    starts_after: Option<String>,
    /// Only show events starting before this date (RFC3339 or YYYY-MM-DD).
    #[arg(long)]
    starts_before: Option<String>,
    /// Case-insensitive substring search in summary and description.
    #[arg(long)]
    search: Option<String>,
    /// Filter events that have a specific property key=value pair.
    /// If just KEY is given (no =), filter events that have that key with any value.
    #[arg(long)]
    has_property: Option<String>,
    /// Only print the number of matching events (for scripting).
    #[arg(long, default_value_t = false)]
    count: bool,
    /// Output format: "default" or "table".
    #[arg(long, value_enum, default_value_t = ListFormat::Default)]
    format: ListFormat,
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

// --- Helper: fetch events for a calendar by name ---

async fn fetch_events(
    client: &GoogleCalendarClient,
    calendar_name: Option<&str>,
    config: &Config,
) -> Result<(String, Vec<models::CalendarEvent>)> {
    let calendars = client.list_calendars().await?;
    let calendar_id = resolve_calendar_id(&calendars, calendar_name, config)?.to_string();
    let events = client.list_all_events(&calendar_id).await?;
    Ok((calendar_id, events))
}

// --- Filter helpers ---

/// Parse a date string (RFC3339 or YYYY-MM-DD) into a chrono DateTime<chrono::FixedOffset>.
/// For YYYY-MM-DD, midnight UTC is used.
fn parse_filter_date(s: &str) -> Result<chrono::DateTime<chrono::FixedOffset>> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Ok(dt);
    }
    let nd = NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .with_context(|| format!("cannot parse '{s}' as RFC3339 or YYYY-MM-DD"))?;
    let ndt = nd.and_hms_opt(0, 0, 0).unwrap();
    Ok(chrono::DateTime::<chrono::FixedOffset>::from_naive_utc_and_offset(
        ndt,
        chrono::FixedOffset::east_opt(0).unwrap(),
    ))
}

/// Extract an event's start time as a DateTime for comparison.
fn event_start_to_datetime(edt: &models::EventDateTime) -> Option<chrono::DateTime<chrono::FixedOffset>> {
    if let Some(ref dt_str) = edt.date_time {
        if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(dt_str) {
            return Some(dt);
        }
    }
    if let Some(ref d_str) = edt.date {
        if let Ok(nd) = NaiveDate::parse_from_str(d_str, "%Y-%m-%d") {
            let ndt = nd.and_hms_opt(0, 0, 0).unwrap();
            return Some(chrono::DateTime::<chrono::FixedOffset>::from_naive_utc_and_offset(
                ndt,
                chrono::FixedOffset::east_opt(0).unwrap(),
            ));
        }
    }
    None
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

    if let Command::Version = &cli.command {
        println!("rscalendar {} by {}", env!("CARGO_PKG_VERSION"), env!("CARGO_PKG_AUTHORS"));
        println!("GIT_DESCRIBE: {}", env!("GIT_DESCRIBE"));
        println!("GIT_SHA: {}", env!("GIT_SHA"));
        println!("GIT_BRANCH: {}", env!("GIT_BRANCH"));
        println!("GIT_DIRTY: {}", env!("GIT_DIRTY"));
        println!("RUSTC_SEMVER: {}", env!("RUSTC_SEMVER"));
        println!("RUST_EDITION: {}", env!("RUST_EDITION"));
        println!("BUILD_TIMESTAMP: {}", env!("BUILD_TIMESTAMP"));
        return Ok(());
    }

    if let Command::Defconfig = &cli.command {
        print!(
            "\
# Default calendar name
# calendar_name = \"My Calendar\"

# Don't open browser during auth (useful for headless machines)
# no_browser = false

# Allowed properties for events
# [properties]
# type = [\"teaching\", \"working\", \"call\", \"meeting\"]
# company = [\"Amdocs\", \"Intel\", \"Google\"]
# course = [\"Linux Fundamentals\", \"Advanced Python\"]

# Required properties per event type (used by 'check' command)
# [check]
# teaching = [\"client\", \"company\", \"course\"]
# working = [\"client\", \"company\"]
"
        );
        return Ok(());
    }

    if let Command::Auth(args) = &cli.command {
        auth::cmd_auth(args.no_browser, args.force, &config).await?;
        return Ok(());
    }

    let client = GoogleCalendarClient::from_cache().await?;

    match cli.command {
        Command::ListCalendars => {
            let calendars = client.list_calendars().await?;
            if calendars.is_empty() {
                if !cli.quiet { println!("No calendars found."); }
            } else {
                for cal in &calendars {
                    print_calendar(cal, cli.json);
                }
            }
        }
        Command::List(args) => {
            let (_, events) = fetch_events(&client, args.calendar_name.as_deref(), &config).await?;

            // Parse filter dates once
            let starts_after = args.starts_after.as_deref().map(parse_filter_date)
                .transpose().context("invalid --starts-after date")?;
            let starts_before = args.starts_before.as_deref().map(parse_filter_date)
                .transpose().context("invalid --starts-before date")?;
            let search_lower = args.search.as_ref().map(|s| s.to_lowercase());
            let has_property = args.has_property.as_deref().map(|s| {
                if let Some((k, v)) = s.split_once('=') {
                    (k.to_string(), Some(v.to_string()))
                } else {
                    (s.to_string(), None)
                }
            });

            let filtered: Vec<_> = events.iter().filter(|event| {
                // --starts-after filter
                if let Some(ref after) = starts_after {
                    if let Some(start) = &event.start {
                        let event_dt = event_start_to_datetime(start);
                        if let Some(evt) = event_dt {
                            if evt <= *after {
                                return false;
                            }
                        }
                    }
                }
                // --starts-before filter
                if let Some(ref before) = starts_before {
                    if let Some(start) = &event.start {
                        let event_dt = event_start_to_datetime(start);
                        if let Some(evt) = event_dt {
                            if evt >= *before {
                                return false;
                            }
                        }
                    }
                }
                // --search filter
                if let Some(ref pattern) = search_lower {
                    let summary_match = event.summary.as_ref()
                        .is_some_and(|s| s.to_lowercase().contains(pattern));
                    let desc_match = event.description.as_ref()
                        .is_some_and(|d| d.to_lowercase().contains(pattern));
                    if !summary_match && !desc_match {
                        return false;
                    }
                }
                // --has-property filter
                if let Some((ref key, ref val)) = has_property {
                    let shared = event.extended_properties.as_ref()
                        .and_then(|p| p.shared.as_ref());
                    match val {
                        Some(expected) => {
                            if !shared.is_some_and(|s| s.get(key).is_some_and(|v| v == expected)) {
                                return false;
                            }
                        }
                        None => {
                            if !shared.is_some_and(|s| s.contains_key(key)) {
                                return false;
                            }
                        }
                    }
                }
                true
            }).collect();

            if args.count {
                println!("{}", filtered.len());
            } else if filtered.is_empty() {
                if !cli.quiet { println!("No events found."); }
            } else {
                match args.format {
                    ListFormat::Table => {
                        println!("{:<30} {:<12} {:<12} {:<15} {:<15}",
                            "SUMMARY", "START", "END", "TYPE", "CLIENT");
                        println!("{}", "-".repeat(84));
                        for event in &filtered {
                            let summary = event.summary_or_default();
                            let start = event.start_str();
                            let end = event.end_str();
                            let shared = event.extended_properties.as_ref()
                                .and_then(|p| p.shared.as_ref());
                            let type_val = shared.and_then(|s| s.get("type"))
                                .map(|s| s.as_str()).unwrap_or("");
                            let client_val = shared.and_then(|s| s.get("client"))
                                .map(|s| s.as_str()).unwrap_or("");
                            // Truncate long values
                            let trunc = |s: &str, max: usize| -> String {
                                if s.len() > max { format!("{}...", &s[..max-3]) } else { s.to_string() }
                            };
                            println!("{:<30} {:<12} {:<12} {:<15} {:<15}",
                                trunc(summary, 30),
                                trunc(&start, 12),
                                trunc(&end, 12),
                                trunc(type_val, 15),
                                trunc(client_val, 15));
                        }
                    }
                    ListFormat::Default => {
                        for event in &filtered {
                            print_event(event, cli.show_builtin, cli.json);
                        }
                    }
                }
            }
        }
        Command::Event { action } => match action {
            EventAction::Create(args) => {
                let calendars = client.list_calendars().await?;
                let calendar_id = resolve_calendar_id(&calendars, args.calendar_name.as_deref(), &config)?;
                let start = parse_event_time(&args.start, false)?;
                let end = parse_event_time(&args.end, true)?;
                let event = client.create_event(calendar_id, &args.summary, &start, &end, args.description.as_deref(), args.location.as_deref()).await?;
                if !cli.quiet { println!("Created event:"); }
                print_event(&event, cli.show_builtin, cli.json);
            }
            EventAction::Update(args) => {
                let calendars = client.list_calendars().await?;
                let calendar_id = resolve_calendar_id(&calendars, args.calendar_name.as_deref(), &config)?;
                let payload = build_event_patch_payload(
                    args.summary.as_deref(),
                    args.start.as_deref(),
                    args.end.as_deref(),
                    args.description.as_deref(),
                    args.location.as_deref(),
                )?;
                let event = client.update_event(calendar_id, &args.event_id, &payload).await?;
                if !cli.quiet { println!("Updated event:"); }
                print_event(&event, cli.show_builtin, cli.json);
            }
            EventAction::Delete(args) => {
                let calendars = client.list_calendars().await?;
                let calendar_id = resolve_calendar_id(&calendars, args.calendar_name.as_deref(), &config)?;
                client.delete_event(calendar_id, &args.event_id).await?;
                if !cli.quiet { println!("Deleted event '{}'.", args.event_id); }
            }
        },
        Command::Check(args) => {
            let check_rules = config.check.as_ref()
                .context("no [check] section in config.toml")?;
            if check_rules.is_empty() {
                bail!("no rules defined in [check] section of config.toml");
            }

            let (_, events) = fetch_events(&client, args.calendar_name.as_deref(), &config).await?;
            if events.is_empty() {
                if !cli.quiet { println!("No events found."); }
                return Ok(());
            }

            let mut issues = 0u32;
            let mut events_with_issues = 0u32;
            for event in &events {
                let summary = event.summary_or_default();
                let start = event.start_str();
                let shared = event.extended_properties.as_ref().and_then(|p| p.shared.as_ref());

                let event_type = match shared.and_then(|s| s.get("type")) {
                    Some(t) => t,
                    None => {
                        println!("{summary} ({start}):");
                        println!("  - missing 'type' property");
                        issues += 1;
                        events_with_issues += 1;
                        continue;
                    }
                };

                let required = match check_rules.get(event_type) {
                    Some(r) => r,
                    None => {
                        println!("{summary} ({start}):");
                        println!("  - unknown type '{event_type}' (not in [check] config)");
                        issues += 1;
                        events_with_issues += 1;
                        continue;
                    }
                };

                let mut event_issues: Vec<String> = Vec::new();
                for key in required {
                    if shared.and_then(|s| s.get(key)).is_none() {
                        event_issues.push(format!("missing required property '{key}' (required for type '{event_type}')"));
                    }
                }

                if !event_issues.is_empty() {
                    println!("{summary} ({start}):");
                    for issue in &event_issues {
                        println!("  - {issue}");
                    }
                    issues += event_issues.len() as u32;
                    events_with_issues += 1;
                }
            }

            if !cli.quiet {
                if issues == 0 {
                    println!("All {} event(s) pass checks.", events.len());
                } else {
                    println!("\n{issues} issue(s) in {events_with_issues} event(s) out of {}.", events.len());
                }
            }
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
                        bail!("value '{value}' is not allowed for key '{key}'. Allowed: {}", allowed.join(", "));
                    }
                }

                let (calendar_id, events) = fetch_events(&client, args.calendar_name.as_deref(), &config).await?;
                if events.is_empty() {
                    if !cli.quiet { println!("No events found."); }
                    return Ok(());
                }

                let sorted_keys: Vec<&String> = {
                    let mut keys: Vec<_> = properties.keys().collect();
                    keys.sort();
                    keys
                };

                if !cli.quiet { println!("Adding properties to {} event(s)\n", events.len()); }

                let mut updated = 0u32;
                let mut skipped = 0u32;
                let mut skipped_no_id = 0u32;
                for event in &events {
                    let summary = event.summary_or_default();
                    let start = event.start_str();
                    let event_id = match &event.id {
                        Some(id) => id,
                        None => { skipped_no_id += 1; continue; }
                    };

                    let existing = event.shared_properties();

                    if let (Some(key), Some(value)) = (&args.key, &args.value) {
                        if existing.get(key).is_some_and(|v| v == value) {
                            skipped += 1;
                            continue;
                        }

                        if !args.all {
                            let prompt = format!("Set {key}={value} on '{summary}' ({start})?");
                            match prompt_yes_no_quit(&prompt)? {
                                Some(true) => {}
                                Some(false) => { skipped += 1; continue; }
                                None => {
                                    if !cli.quiet { println!("\nQuit. {updated} updated, {skipped} skipped."); }
                                    return Ok(());
                                }
                            }
                        }

                        let mut new_props = existing;
                        new_props.insert(key.clone(), value.clone());
                        client.patch_event_properties(&calendar_id, event_id, &new_props).await?;
                        if !cli.quiet { println!("  set on: {summary} ({start})"); }
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
                            client.patch_event_properties(&calendar_id, event_id, &new_props).await?;
                            eprintln!("  updated\n");
                            updated += 1;
                        } else {
                            eprintln!("  no changes\n");
                            skipped += 1;
                        }
                    }
                }

                if skipped_no_id > 0 {
                    eprintln!("Warning: skipped {skipped_no_id} event(s) with no ID");
                }
                if !cli.quiet { println!("Done. {updated} updated, {skipped} skipped."); }
            }
            PropertiesAction::Check(args) => {
                let properties = config.properties.as_ref()
                    .context("no [properties] section in config.toml")?;
                if properties.is_empty() {
                    bail!("no properties defined in [properties] section of config.toml");
                }

                let (_, events) = fetch_events(&client, args.calendar_name.as_deref(), &config).await?;
                if events.is_empty() {
                    if !cli.quiet { println!("No events found."); }
                    return Ok(());
                }

                let mut issues = 0u32;
                for event in &events {
                    let summary = event.summary_or_default();
                    let start = event.start_str();
                    let shared = event.extended_properties.as_ref().and_then(|p| p.shared.as_ref());

                    let mut event_issues: Vec<String> = Vec::new();

                    for key in properties.keys() {
                        match shared.and_then(|s| s.get(key)) {
                            None => event_issues.push(format!("missing property '{key}'")),
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

                if !cli.quiet {
                    if issues == 0 {
                        println!("All {} event(s) have valid properties.", events.len());
                    } else {
                        println!("\n{issues} issue(s) found across {} event(s).", events.len());
                    }
                }
            }
            PropertiesAction::Delete(args) => {
                let (calendar_id, events) = fetch_events(&client, args.calendar_name.as_deref(), &config).await?;
                if events.is_empty() {
                    if !cli.quiet { println!("No events found."); }
                    return Ok(());
                }

                let mut updated = 0u32;
                let mut skipped = 0u32;
                let mut skipped_no_id = 0u32;
                for event in &events {
                    let summary = event.summary_or_default();
                    let start = event.start_str();
                    let event_id = match &event.id {
                        Some(id) => id,
                        None => { skipped_no_id += 1; continue; }
                    };

                    let existing = event.shared_properties();
                    if !existing.contains_key(&args.key) {
                        skipped += 1;
                        continue;
                    }

                    if !args.all {
                        let current_value = &existing[&args.key];
                        let prompt = format!("Delete {}={current_value} from '{summary}' ({start})?", args.key);
                        match prompt_yes_no_quit(&prompt)? {
                            Some(true) => {}
                            Some(false) => { skipped += 1; continue; }
                            None => {
                                if !cli.quiet { println!("\nQuit. {updated} deleted, {skipped} skipped."); }
                                return Ok(());
                            }
                        }
                    }

                    client.delete_property(&calendar_id, event_id, &args.key).await?;
                    if !cli.quiet { println!("  deleted from: {summary} ({start})"); }
                    updated += 1;
                }

                if skipped_no_id > 0 {
                    eprintln!("Warning: skipped {skipped_no_id} event(s) with no ID");
                }
                if !cli.quiet { println!("\nDone. {updated} deleted, {skipped} skipped."); }
            }
            PropertiesAction::Rename(args) => {
                let (calendar_id, events) = fetch_events(&client, args.calendar_name.as_deref(), &config).await?;
                if events.is_empty() {
                    if !cli.quiet { println!("No events found."); }
                    return Ok(());
                }

                let mut updated = 0u32;
                let mut skipped = 0u32;
                let mut skipped_no_id = 0u32;
                for event in &events {
                    let summary = event.summary_or_default();
                    let start = event.start_str();
                    let event_id = match &event.id {
                        Some(id) => id,
                        None => { skipped_no_id += 1; continue; }
                    };

                    let mut existing = event.shared_properties();
                    let value = match existing.remove(&args.from) {
                        Some(v) => v,
                        None => { skipped += 1; continue; }
                    };

                    if !args.all {
                        let prompt = format!(
                            "Rename '{}'='{}' to '{}'='{}' on '{summary}' ({start})?",
                            args.from, value, args.to, value
                        );
                        match prompt_yes_no_quit(&prompt)? {
                            Some(true) => {}
                            Some(false) => { skipped += 1; continue; }
                            None => {
                                if !cli.quiet { println!("\nQuit. {updated} renamed, {skipped} skipped."); }
                                return Ok(());
                            }
                        }
                    }

                    existing.insert(args.to.clone(), value);
                    client.patch_event_properties(&calendar_id, event_id, &existing).await?;
                    if !cli.quiet { println!("  renamed on: {summary} ({start})"); }
                    updated += 1;
                }

                if skipped_no_id > 0 {
                    eprintln!("Warning: skipped {skipped_no_id} event(s) with no ID");
                }
                if !cli.quiet { println!("\nDone. {updated} renamed, {skipped} skipped."); }
            }
            PropertiesAction::Edit(args) => {
                let properties = config.properties.as_ref()
                    .context("no [properties] section in config.toml")?;
                if properties.is_empty() {
                    bail!("no properties defined in [properties] section of config.toml");
                }

                let (calendar_id, events) = fetch_events(&client, args.calendar_name.as_deref(), &config).await?;
                if events.is_empty() {
                    if !cli.quiet { println!("No events found."); }
                    return Ok(());
                }

                let sorted_keys: Vec<&String> = {
                    let mut keys: Vec<_> = properties.keys().collect();
                    keys.sort();
                    keys
                };

                let mut updated = 0u32;
                let mut skipped_no_id = 0u32;
                for event in &events {
                    let summary = event.summary_or_default();
                    let start = event.start_str();
                    let end = event.end_str();
                    let event_id = match &event.id {
                        Some(id) => id,
                        None => { skipped_no_id += 1; continue; }
                    };

                    let mut current = event.shared_properties();
                    let mut changed = false;
                    let mut deleted_keys: Vec<String> = Vec::new();

                    loop {
                        eprintln!();
                        eprintln!("{summary}");
                        eprintln!("  start: {start}");
                        eprintln!("  end:   {end}");
                        if let Some(location) = &event.location {
                            eprintln!("  location: {location}");
                        }
                        if let Some(description) = &event.description {
                            eprintln!("  description: {description}");
                        }
                        if current.is_empty() && deleted_keys.is_empty() {
                            eprintln!("  (no properties)");
                        } else {
                            for key in &sorted_keys {
                                if let Some(val) = current.get(*key) {
                                    eprintln!("  {key}: {val}");
                                }
                            }
                            for (k, v) in &current {
                                if !properties.contains_key(k) {
                                    eprintln!("  {k}: {v} (unknown)");
                                }
                            }
                        }

                        struct MenuEntry { key: String, actions: Vec<(&'static str, &'static str)> }
                        let mut menu_entries: Vec<MenuEntry> = Vec::new();

                        for key in &sorted_keys {
                            if current.contains_key(*key) {
                                menu_entries.push(MenuEntry {
                                    key: (*key).clone(),
                                    actions: vec![("c", "change"), ("d", "delete")],
                                });
                            } else {
                                menu_entries.push(MenuEntry {
                                    key: (*key).clone(),
                                    actions: vec![("a", "add")],
                                });
                            }
                        }

                        eprintln!("  Actions:");
                        for (i, entry) in menu_entries.iter().enumerate() {
                            let actions_str: Vec<String> = entry.actions.iter()
                                .map(|(code, label)| format!("{code}={label}"))
                                .collect();
                            eprintln!("    {}: '{}' [{}]", i + 1, entry.key, actions_str.join(", "));
                        }
                        eprintln!("    n: next event");
                        eprintln!("    q: quit");

                        use std::io::{BufRead, Write};
                        eprint!("  choice: ");
                        std::io::stderr().flush()?;
                        let mut line = String::new();
                        std::io::stdin().lock().read_line(&mut line)?;
                        let trimmed = line.trim().to_lowercase();

                        if trimmed == "n" || trimmed == "next" {
                            break;
                        }
                        if trimmed == "q" || trimmed == "quit" {
                            if changed {
                                client.patch_event_properties_with_deletes(&calendar_id, event_id, &current, &deleted_keys).await?;
                                updated += 1;
                                eprintln!("  saved.");
                            }
                            if !cli.quiet { println!("\nQuit. {updated} event(s) updated."); }
                            return Ok(());
                        }

                        let (num_str, action_code) = if trimmed.len() >= 2 && trimmed.as_bytes().last().unwrap().is_ascii_alphabetic() {
                            (&trimmed[..trimmed.len()-1], Some(&trimmed[trimmed.len()-1..]))
                        } else {
                            (trimmed.as_str(), None)
                        };

                        let idx = match num_str.parse::<usize>() {
                            Ok(n) if n >= 1 && n <= menu_entries.len() => n - 1,
                            _ => { eprintln!("  invalid choice"); continue; }
                        };

                        let entry = &menu_entries[idx];
                        let action = if entry.actions.len() == 1 {
                            entry.actions[0].0
                        } else if let Some(code) = action_code {
                            if let Some((a, _)) = entry.actions.iter().find(|(c, _)| *c == code) {
                                a
                            } else {
                                eprintln!("  invalid action. Use: {}", entry.actions.iter().map(|(c, l)| format!("{c}={l}")).collect::<Vec<_>>().join(", "));
                                continue;
                            }
                        } else {
                            eprintln!("  specify action: {}", entry.actions.iter().map(|(c, l)| format!("{c}={l}")).collect::<Vec<_>>().join(", "));
                            continue;
                        };

                        let key = &entry.key;
                        match action {
                            "a" | "c" => {
                                let values = &properties[key];
                                let prompt = format!("  Select value for '{key}':");
                                if let Some(value) = prompt_select(&prompt, values)? {
                                    current.insert(key.clone(), value);
                                    changed = true;
                                }
                            }
                            "d" => {
                                current.remove(key);
                                deleted_keys.push(key.clone());
                                changed = true;
                                eprintln!("  deleted '{key}'");
                            }
                            _ => unreachable!(),
                        }
                    }

                    if changed {
                        client.patch_event_properties_with_deletes(&calendar_id, event_id, &current, &deleted_keys).await?;
                        eprintln!("  saved.");
                        updated += 1;
                    }
                }

                if skipped_no_id > 0 {
                    eprintln!("Warning: skipped {skipped_no_id} event(s) with no ID");
                }
                if !cli.quiet { println!("\nDone. {updated} event(s) updated."); }
            }
            PropertiesAction::SetValue(args) => {
                let (calendar_id, events) = fetch_events(&client, args.calendar_name.as_deref(), &config).await?;
                if events.is_empty() {
                    if !cli.quiet { println!("No events found."); }
                    return Ok(());
                }

                let mut updated = 0u32;
                let mut skipped = 0u32;
                let mut skipped_no_id = 0u32;
                for event in &events {
                    let summary = event.summary_or_default();
                    let start = event.start_str();
                    let event_id = match &event.id {
                        Some(id) => id,
                        None => { skipped_no_id += 1; continue; }
                    };

                    let existing = event.shared_properties();
                    match existing.get(&args.key) {
                        Some(v) if v == &args.from => {}
                        _ => { skipped += 1; continue; }
                    }

                    if !args.all {
                        let prompt = format!(
                            "Change {}='{}' to '{}' on '{summary}' ({start})?",
                            args.key, args.from, args.to
                        );
                        match prompt_yes_no_quit(&prompt)? {
                            Some(true) => {}
                            Some(false) => { skipped += 1; continue; }
                            None => {
                                if !cli.quiet { println!("\nQuit. {updated} changed, {skipped} skipped."); }
                                return Ok(());
                            }
                        }
                    }

                    let mut new_props = existing;
                    new_props.insert(args.key.clone(), args.to.clone());
                    client.patch_event_properties(&calendar_id, event_id, &new_props).await?;
                    if !cli.quiet { println!("  changed on: {summary} ({start})"); }
                    updated += 1;
                }

                if skipped_no_id > 0 {
                    eprintln!("Warning: skipped {skipped_no_id} event(s) with no ID");
                }
                if !cli.quiet { println!("\nDone. {updated} changed, {skipped} skipped."); }
            }
        },
        Command::Calendar { action } => match action {
            CalendarAction::Create { name } => {
                let calendar = client.create_calendar(&name).await?;
                let id = calendar["id"].as_str().unwrap_or("<unknown>");
                if !cli.quiet {
                    println!("Created public calendar '{name}'");
                    println!("  id: {id}");
                }
            }
            CalendarAction::Delete { name } => {
                let calendars = client.list_calendars().await?;
                let calendar_id = resolve_calendar_id(&calendars, Some(&name), &config)?;
                use std::io::{BufRead, Write};
                eprint!("Delete calendar '{name}'? This cannot be undone. [y/n]: ");
                std::io::stderr().flush()?;
                let mut line = String::new();
                std::io::stdin().lock().read_line(&mut line)?;
                match line.trim().to_lowercase().as_str() {
                    "y" | "yes" => {
                        client.delete_calendar(calendar_id).await?;
                        if !cli.quiet { println!("Deleted calendar '{name}'."); }
                    }
                    _ => {
                        if !cli.quiet { println!("Aborted."); }
                    }
                }
            }
            CalendarAction::Rename { name, new_name } => {
                let calendars = client.list_calendars().await?;
                let calendar_id = resolve_calendar_id(&calendars, Some(&name), &config)?;
                client.rename_calendar(calendar_id, &new_name).await?;
                if !cli.quiet { println!("Renamed calendar '{name}' to '{new_name}'."); }
            }
            CalendarAction::Clear(args) => {
                let (calendar_id, events) = fetch_events(&client, args.calendar_name.as_deref(), &config).await?;
                if events.is_empty() {
                    if !cli.quiet { println!("No events found."); }
                    return Ok(());
                }

                if !cli.quiet {
                    println!("Found {} event(s)", events.len());
                    if !args.all {
                        println!("  interactive mode: y=delete, n=skip, q=quit");
                    }
                    println!();
                }

                let mut deleted = 0u32;
                let mut skipped = 0u32;
                for event in &events {
                    let summary = event.summary_or_default();
                    let start = event.start_str();
                    let event_id = match &event.id {
                        Some(id) => id,
                        None => {
                            eprintln!("  skipping event with no id: {summary}");
                            skipped += 1;
                            continue;
                        }
                    };

                    if !args.all {
                        let prompt = format!("  Delete '{summary}' ({start})?");
                        match prompt_yes_no_quit(&prompt)? {
                            Some(true) => {}
                            Some(false) => { skipped += 1; continue; }
                            None => {
                                if !cli.quiet { println!("\nQuit. {deleted} event(s) deleted, {skipped} skipped."); }
                                return Ok(());
                            }
                        }
                    }

                    client.delete_event(&calendar_id, event_id).await?;
                    if !cli.quiet { println!("  deleted: {summary} ({start})"); }
                    deleted += 1;
                }

                if !cli.quiet {
                    println!("\nDone. {deleted} event(s) deleted, {skipped} skipped.");
                }
            }
            CalendarAction::Copy(args) => {
                let calendars = client.list_calendars().await?;

                let source_cal = calendars.iter()
                    .find(|c| c.summary.as_deref() == Some(&args.source))
                    .context(format!("no calendar named '{}' found", args.source))?;
                let source_id = source_cal.id.as_deref().context("source calendar has no id")?.to_string();

                let target_cal = calendars.iter()
                    .find(|c| c.summary.as_deref() == Some(&args.target))
                    .context(format!("no calendar named '{}' found", args.target))?;
                let target_id = target_cal.id.as_deref().context("target calendar has no id")?.to_string();

                if !cli.quiet { println!("Copying events from '{}' to '{}'\n", args.source, args.target); }

                let events = client.list_all_events(&source_id).await?;
                if events.is_empty() {
                    if !cli.quiet { println!("No events found in '{}'.", args.source); }
                    return Ok(());
                }

                if !cli.quiet { println!("Found {} event(s)\n", events.len()); }

                let mut copied = 0u32;
                let mut skipped = 0u32;
                for event in &events {
                    let summary = event.summary_or_default();
                    let start = event.start_str();
                    if event.id.is_none() {
                        eprintln!("  skipping event with no id: {summary}");
                        skipped += 1;
                        continue;
                    }

                    if args.dry_run {
                        if !cli.quiet { println!("  [dry-run] would copy: {summary} ({start})"); }
                        copied += 1;
                    } else {
                        let mut payload = json!({ "summary": summary });
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
                        if let Some(props) = &event.extended_properties {
                            payload["extendedProperties"] = serde_json::to_value(props)?;
                        }

                        client.insert_event_raw(&target_id, &payload).await?;
                        if !cli.quiet { println!("  copied: {summary} ({start})"); }
                        copied += 1;
                    }
                }

                if !cli.quiet {
                    if args.dry_run {
                        println!("\nDry run complete. {copied} event(s) would be copied, {skipped} skipped.");
                    } else {
                        println!("\nDone. {copied} event(s) copied to '{}', {skipped} skipped.", args.target);
                    }
                }
            }
        },
        Command::MoveEvents(args) => {
            let calendars = client.list_calendars().await?;

            let source_cal = calendars.iter()
                .find(|c| c.summary.as_deref() == Some(&args.source))
                .context(format!("no calendar named '{}' found", args.source))?;
            let source_id = source_cal.id.as_deref().context("source calendar has no id")?.to_string();

            let target_cal = calendars.iter()
                .find(|c| c.summary.as_deref() == Some(&args.target))
                .context(format!("no calendar named '{}' found", args.target))?;
            let target_id = target_cal.id.as_deref().context("target calendar has no id")?.to_string();

            if !cli.quiet { println!("Moving events from '{}' to '{}'", args.source, args.target); }
            if !cli.quiet {
                if !args.all {
                    println!("  interactive mode: y=move, n=skip, q=quit");
                }
                println!();
            }

            let events = client.list_all_events(&source_id).await?;
            if events.is_empty() {
                if !cli.quiet { println!("No events found in '{}'.", args.source); }
                return Ok(());
            }

            if !cli.quiet { println!("Found {} event(s)\n", events.len()); }

            let mut moved = 0u32;
            let mut skipped = 0u32;
            for event in &events {
                let summary = event.summary_or_default();
                let start = event.start_str();
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
                        Some(false) => { skipped += 1; continue; }
                        None => {
                            if !cli.quiet { println!("\nQuit. {moved} event(s) moved, {skipped} skipped."); }
                            return Ok(());
                        }
                    }
                }

                if args.dry_run {
                    if !cli.quiet { println!("  [dry-run] would move: {summary} ({start})"); }
                    moved += 1;
                } else {
                    let mut payload = json!({ "summary": summary });
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
                    if let Some(props) = &event.extended_properties {
                        payload["extendedProperties"] = serde_json::to_value(props)?;
                    }

                    client.insert_event_raw(&target_id, &payload).await?;
                    client.delete_event(&source_id, event_id).await?;
                    if !cli.quiet { println!("  moved: {summary} ({start})"); }
                    moved += 1;
                }
            }

            if !cli.quiet {
                if args.dry_run {
                    println!("\nDry run complete. {moved} event(s) would be moved, {skipped} skipped.");
                } else {
                    println!("\nDone. {moved} event(s) moved to '{}', {skipped} skipped.", args.target);
                }
            }
        }
        Command::Stats(args) => {
            let (_, events) = fetch_events(&client, args.calendar_name.as_deref(), &config).await?;

            println!("Total events: {}", events.len());

            let mut by_type: BTreeMap<String, u32> = BTreeMap::new();
            let mut by_client: BTreeMap<String, u32> = BTreeMap::new();
            let mut by_month: BTreeMap<String, u32> = BTreeMap::new();

            for event in &events {
                let shared = event.extended_properties.as_ref()
                    .and_then(|p| p.shared.as_ref());
                let type_val = shared.and_then(|s| s.get("type"))
                    .cloned().unwrap_or_else(|| "(no type)".to_string());
                let client_val = shared.and_then(|s| s.get("client"))
                    .cloned().unwrap_or_else(|| "(no client)".to_string());

                *by_type.entry(type_val).or_insert(0) += 1;
                *by_client.entry(client_val).or_insert(0) += 1;

                if let Some(start) = &event.start {
                    let month_str = if let Some(ref dt_str) = start.date_time {
                        dt_str.get(..7).map(|s| s.to_string())
                    } else if let Some(ref d_str) = start.date {
                        d_str.get(..7).map(|s| s.to_string())
                    } else {
                        None
                    };
                    if let Some(m) = month_str {
                        *by_month.entry(m).or_insert(0) += 1;
                    }
                }
            }

            println!("\nEvents by type:");
            for (k, v) in &by_type {
                println!("  {k}: {v}");
            }

            println!("\nEvents by client:");
            for (k, v) in &by_client {
                println!("  {k}: {v}");
            }

            println!("\nEvents by month:");
            for (k, v) in &by_month {
                println!("  {k}: {v}");
            }
        }
        Command::Auth(_) | Command::Complete { .. } | Command::Defconfig | Command::Version => unreachable!(),
    }

    Ok(())
}
