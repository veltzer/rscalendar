mod auth;
mod cli;
mod client;
mod commands;
mod config;
mod helpers;
mod models;

use anyhow::Result;
use clap::{CommandFactory, Parser};

use cli::{CalendarAction, Cli, Command, EventAction, OutputOptions, PropertiesAction};
use client::GoogleCalendarClient;
use config::Config;

// --- Helper: fetch events for a calendar by name ---

pub async fn fetch_events(
    client: &GoogleCalendarClient,
    calendar_name: Option<&str>,
    config: &Config,
) -> Result<(String, Vec<models::CalendarEvent>)> {
    let calendars = client.list_calendars().await?;
    let calendar_id = client::resolve_calendar_id(&calendars, calendar_name, config)?.to_string();
    let events = client.list_all_events(&calendar_id).await?;
    Ok((calendar_id, events))
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
    let out = OutputOptions {
        show_builtin: cli.show_builtin,
        json: cli.json,
        quiet: cli.quiet,
    };

    match cli.command {
        Command::ListCalendars => {
            commands::list::cmd_list_calendars(&client, &out).await?;
        }
        Command::List(args) => {
            commands::list::cmd_list(&client, &args, &config, &out).await?;
        }
        Command::Event { action } => match action {
            EventAction::Create(args) => {
                commands::event::cmd_event_create(&client, &args, &config, &out).await?;
            }
            EventAction::Update(args) => {
                commands::event::cmd_event_update(&client, &args, &config, &out).await?;
            }
            EventAction::Delete(args) => {
                commands::event::cmd_event_delete(&client, &args, &config, &out).await?;
            }
            EventAction::Edit(args) => {
                commands::event::cmd_event_edit(&client, &args, &config, &out).await?;
            }
        },
        Command::Check(args) => {
            commands::check::cmd_check(&client, &args, &config, &out).await?;
        }
        Command::Properties { action } => match action {
            PropertiesAction::Add(args) => {
                commands::properties::cmd_properties_add(&client, &args, &config, &out).await?;
            }
            PropertiesAction::Check(args) => {
                commands::properties::cmd_properties_check(&client, &args, &config, &out).await?;
            }
            PropertiesAction::Delete(args) => {
                commands::properties::cmd_properties_delete(&client, &args, &config, &out).await?;
            }
            PropertiesAction::Rename(args) => {
                commands::properties::cmd_properties_rename(&client, &args, &config, &out).await?;
            }
            PropertiesAction::Edit(args) => {
                commands::properties::cmd_properties_edit(&client, &args, &config, &out).await?;
            }
            PropertiesAction::SetValue(args) => {
                commands::properties::cmd_properties_set_value(&client, &args, &config, &out).await?;
            }
        },
        Command::Calendar { action } => match action {
            CalendarAction::Create { name } => {
                commands::calendar::cmd_calendar_create(&client, &name, &out).await?;
            }
            CalendarAction::Delete { name } => {
                commands::calendar::cmd_calendar_delete(&client, &name, &config, &out).await?;
            }
            CalendarAction::Rename { name, new_name } => {
                commands::calendar::cmd_calendar_rename(&client, &name, &new_name, &config, &out).await?;
            }
            CalendarAction::Clear(args) => {
                commands::calendar::cmd_calendar_clear(&client, &args, &config, &out).await?;
            }
            CalendarAction::Copy(args) => {
                commands::calendar::cmd_calendar_copy(&client, &args, &out).await?;
            }
        },
        Command::MoveEvents(args) => {
            commands::move_events::cmd_move_events(&client, &args, &out).await?;
        }
        Command::Stats(args) => {
            commands::list::cmd_stats(&client, &args, &config, &out).await?;
        }
        Command::Auth(_) | Command::Complete { .. } | Command::Defconfig | Command::Version => unreachable!(),
    }

    Ok(())
}
