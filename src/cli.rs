use clap::{Args, Parser, Subcommand, ValueEnum};

// --- CLI ---

#[derive(Debug, Parser)]
#[command(
    author,
    version,
    about = "Google Calendar CLI for listing and changing events"
)]
pub struct Cli {
    /// Show "(built-in)" labels on standard Google Calendar fields.
    #[arg(long, global = true, default_value_t = false)]
    pub show_builtin: bool,

    /// Output as JSON instead of human-readable text.
    #[arg(long, global = true, default_value_t = false)]
    pub json: bool,

    /// Suppress informational output; only show data and errors.
    #[arg(long, global = true, default_value_t = false)]
    pub quiet: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
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
pub enum CalendarAction {
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
pub enum EventAction {
    /// Create a new event.
    Create(UpsertArgs),
    /// Update fields on an existing event.
    Update(UpdateArgs),
    /// Delete an event.
    Delete(DeleteArgs),
    /// Interactively edit an event's fields and properties.
    Edit(EventEditArgs),
}

#[derive(Debug, Args)]
pub struct EventEditArgs {
    /// Calendar name (default: from config).
    #[arg(long)]
    pub calendar_name: Option<String>,
    /// Event ID to edit.
    #[arg(long)]
    pub event_id: String,
}

#[derive(Debug, Subcommand)]
pub enum PropertiesAction {
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
pub struct CalendarNameArgs {
    /// Calendar name (default: from config).
    #[arg(long)]
    pub calendar_name: Option<String>,
}

#[derive(Debug, Args)]
pub struct CheckArgs {
    /// Calendar name (default: from config).
    #[arg(long)]
    pub calendar_name: Option<String>,
    /// Interactively fix events that have issues by prompting to set missing properties.
    #[arg(long, default_value_t = false)]
    pub fix: bool,
}

#[derive(Debug, Args)]
pub struct MoveEventsArgs {
    /// Source calendar name to move events from.
    #[arg(long)]
    pub source: String,
    /// Target calendar name to move events into.
    #[arg(long)]
    pub target: String,
    /// Show what would be done without making changes.
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
    /// Move all events without prompting.
    #[arg(long, default_value_t = false)]
    pub all: bool,
}

#[derive(Debug, Args)]
pub struct PropertiesAddArgs {
    /// Calendar name (default: from config).
    #[arg(long)]
    pub calendar_name: Option<String>,
    /// Property key to set (must be defined in config). If omitted, prompts for all missing properties.
    #[arg(long)]
    pub key: Option<String>,
    /// Property value to set (must be allowed for the key in config). Requires --key.
    #[arg(long)]
    pub value: Option<String>,
    /// Apply to all events without prompting.
    #[arg(long, default_value_t = false)]
    pub all: bool,
}

#[derive(Debug, Args)]
pub struct PropertiesDeleteArgs {
    /// Calendar name (default: from config).
    #[arg(long)]
    pub calendar_name: Option<String>,
    /// Property key to delete.
    #[arg(long)]
    pub key: String,
    /// Apply to all events without prompting.
    #[arg(long, default_value_t = false)]
    pub all: bool,
}

#[derive(Debug, Args)]
pub struct PropertiesRenameArgs {
    /// Calendar name (default: from config).
    #[arg(long)]
    pub calendar_name: Option<String>,
    /// Current property key name.
    #[arg(long)]
    pub from: String,
    /// New property key name.
    #[arg(long)]
    pub to: String,
    /// Apply to all events without prompting.
    #[arg(long, default_value_t = false)]
    pub all: bool,
}

#[derive(Debug, Args)]
pub struct PropertiesSetValueArgs {
    /// Calendar name (default: from config).
    #[arg(long)]
    pub calendar_name: Option<String>,
    /// Property key to change.
    #[arg(long)]
    pub key: String,
    /// Current value to match.
    #[arg(long)]
    pub from: String,
    /// New value to set.
    #[arg(long)]
    pub to: String,
    /// Apply to all matching events without prompting.
    #[arg(long, default_value_t = false)]
    pub all: bool,
}

#[derive(Debug, Args)]
pub struct CalendarClearArgs {
    /// Calendar name (default: from config).
    #[arg(long)]
    pub calendar_name: Option<String>,
    /// Delete all events without prompting.
    #[arg(long, default_value_t = false)]
    pub all: bool,
}

#[derive(Debug, Args)]
pub struct CalendarCopyArgs {
    /// Source calendar name to copy events from.
    #[arg(long)]
    pub source: String,
    /// Target calendar name to copy events into.
    #[arg(long)]
    pub target: String,
    /// Show what would be done without making changes.
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
}

#[derive(Debug, Clone, ValueEnum)]
pub enum ListFormat {
    /// Default human-readable output.
    Default,
    /// Aligned table with columns: summary, start, end, type, client.
    Table,
}

#[derive(Debug, Args)]
pub struct ListArgs {
    /// Calendar name (default: from config).
    #[arg(long)]
    pub calendar_name: Option<String>,
    /// Only show events starting after this date (RFC3339 or YYYY-MM-DD).
    #[arg(long)]
    pub starts_after: Option<String>,
    /// Only show events starting before this date (RFC3339 or YYYY-MM-DD).
    #[arg(long)]
    pub starts_before: Option<String>,
    /// Case-insensitive substring search in summary and description.
    #[arg(long)]
    pub search: Option<String>,
    /// Filter events that have a specific property key=value pair.
    /// If just KEY is given (no =), filter events that have that key with any value.
    #[arg(long)]
    pub has_property: Option<String>,
    /// Only print the number of matching events (for scripting).
    #[arg(long, default_value_t = false)]
    pub count: bool,
    /// Output format: "default" or "table".
    #[arg(long, value_enum, default_value_t = ListFormat::Default)]
    pub format: ListFormat,
}

#[derive(Debug, Args)]
pub struct UpsertArgs {
    /// Calendar name (default: from config).
    #[arg(long)]
    pub calendar_name: Option<String>,
    /// Event summary/title.
    #[arg(long)]
    pub summary: String,
    /// Event start time. Accepts RFC3339 or YYYY-MM-DD.
    #[arg(long)]
    pub start: String,
    /// Event end time. Accepts RFC3339 or YYYY-MM-DD. For all-day events,
    /// the provided date is treated as inclusive and converted to Google's
    /// exclusive end-date format.
    #[arg(long)]
    pub end: String,
    /// Optional description/body text.
    #[arg(long)]
    pub description: Option<String>,
    /// Optional location string.
    #[arg(long)]
    pub location: Option<String>,
}

#[derive(Debug, Args)]
pub struct UpdateArgs {
    /// Calendar name (default: from config).
    #[arg(long)]
    pub calendar_name: Option<String>,
    /// Event ID to update.
    #[arg(long)]
    pub event_id: String,
    /// Replacement event summary/title.
    #[arg(long)]
    pub summary: Option<String>,
    /// Replacement start time. Accepts RFC3339 or YYYY-MM-DD.
    #[arg(long)]
    pub start: Option<String>,
    /// Replacement end time. Accepts RFC3339 or YYYY-MM-DD.
    #[arg(long)]
    pub end: Option<String>,
    /// Replacement description.
    #[arg(long)]
    pub description: Option<String>,
    /// Replacement location.
    #[arg(long)]
    pub location: Option<String>,
}

#[derive(Debug, Args)]
pub struct DeleteArgs {
    /// Calendar name (default: from config).
    #[arg(long)]
    pub calendar_name: Option<String>,
    /// Event ID to delete.
    #[arg(long)]
    pub event_id: String,
}

#[derive(Debug, Args)]
pub struct AuthArgs {
    /// Print the authorization URL instead of opening the browser.
    #[arg(long, default_value_t = false)]
    pub no_browser: bool,
    /// Force re-authentication by removing cached token first.
    #[arg(long, default_value_t = false)]
    pub force: bool,
}

pub struct OutputOptions {
    pub show_builtin: bool,
    pub json: bool,
    pub quiet: bool,
}
