use anyhow::{Context, Result, bail};

use crate::cli::{CheckArgs, OutputOptions};
use crate::client::GoogleCalendarClient;
use crate::config::Config;
use crate::fetch_events;
use crate::helpers::{prompt_select, prompt_yes_no_quit};

pub async fn cmd_check(
    client: &GoogleCalendarClient,
    args: &CheckArgs,
    config: &Config,
    out: &OutputOptions,
) -> Result<()> {
    let check_rules = config
        .check
        .as_ref()
        .context("no [check] section in config.toml")?;
    if check_rules.is_empty() {
        bail!("no rules defined in [check] section of config.toml");
    }

    let properties = if args.fix {
        Some(
            config
                .properties
                .as_ref()
                .context("--fix requires [properties] section in config.toml")?,
        )
    } else {
        None
    };

    let (calendar_id, events) = fetch_events(client, args.calendar_name.as_deref(), config).await?;
    if events.is_empty() {
        if !out.quiet {
            println!("No events found.");
        }
        return Ok(());
    }

    let mut issues = 0u32;
    let mut events_with_issues = 0u32;
    let mut fixed = 0u32;
    for event in &events {
        let summary = event.summary_or_default();
        let start = event.start_str();
        let shared = event
            .extended_properties
            .as_ref()
            .and_then(|p| p.shared.as_ref());
        let event_id = event.id.as_deref();

        let event_type = if let Some(t) = shared.and_then(|s| s.get("type")) {
            t.clone()
        } else {
            println!("{summary} ({start}):");
            println!("- missing 'type' property");
            issues += 1;
            events_with_issues += 1;

            if args.fix
                && let (Some(props), Some(eid)) = (&properties, event_id)
                && fix_missing_type(client, &calendar_id, event, eid, props, check_rules).await?
            {
                fixed += 1;
            }
            continue;
        };

        let Some(required) = check_rules.get(&event_type) else {
            println!("{summary} ({start}):");
            println!("- unknown type '{event_type}' (not in [check] config)");
            issues += 1;
            events_with_issues += 1;
            continue;
        };

        let missing_keys: Vec<&String> = required
            .iter()
            .filter(|key| shared.and_then(|s| s.get(*key)).is_none())
            .collect();

        if !missing_keys.is_empty() {
            println!("{summary} ({start}):");
            for key in &missing_keys {
                println!("- missing required property '{key}' (required for type '{event_type}')");
            }
            issues += u32::try_from(missing_keys.len()).unwrap_or(u32::MAX);
            events_with_issues += 1;

            if args.fix
                && let (Some(props), Some(eid)) = (&properties, event_id)
            {
                match prompt_yes_no_quit("Fix this event?")? {
                    Some(true) => {
                        fix_missing_keys(client, &calendar_id, event, eid, props, &missing_keys)
                            .await?;
                        fixed += 1;
                    }
                    Some(false) => {}
                    None => {
                        if !out.quiet {
                            println!("\nQuit. {fixed} event(s) fixed.");
                        }
                        return Ok(());
                    }
                }
            }
        }
    }

    if !out.quiet {
        print_summary(issues, events_with_issues, events.len(), fixed, args.fix);
    }

    Ok(())
}

/// Print the closing summary of a check run.
fn print_summary(issues: u32, events_with_issues: u32, total: usize, fixed: u32, fix: bool) {
    if issues == 0 {
        println!("All {total} event(s) pass checks.");
    } else {
        println!("\n{issues} issue(s) in {events_with_issues} event(s) out of {total}.");
        if fix {
            println!("{fixed} event(s) fixed.");
        }
    }
}

type Rules = std::collections::HashMap<String, Vec<String>>;

/// Interactively choose a type for an event that lacks one and patch it.
/// Returns whether the event was fixed.
async fn fix_missing_type(
    client: &GoogleCalendarClient,
    calendar_id: &str,
    event: &crate::models::CalendarEvent,
    event_id: &str,
    props: &Rules,
    check_rules: &Rules,
) -> Result<bool> {
    let Some(type_values) = props.get("type") else {
        return Ok(false);
    };
    let prompt = format!("Fix: select type for '{}':", event.summary_or_default());
    let Some(chosen_type) = prompt_select(&prompt, type_values)? else {
        return Ok(false);
    };
    let mut new_props = event.shared_properties();
    new_props.insert("type".to_string(), chosen_type.clone());

    // Now check required properties for the chosen type
    if let Some(required) = check_rules.get(&chosen_type) {
        for key in required {
            if !new_props.contains_key(key)
                && let Some(key_values) = props.get(key)
            {
                let key_prompt = format!("Fix: select {key}:");
                if let Some(value) = prompt_select(&key_prompt, key_values)? {
                    new_props.insert(key.clone(), value);
                }
            }
        }
    }

    client
        .patch_event_properties(calendar_id, event_id, &new_props)
        .await?;
    println!("fixed.");
    Ok(true)
}

/// Interactively fill in the missing properties of an event and patch it.
async fn fix_missing_keys(
    client: &GoogleCalendarClient,
    calendar_id: &str,
    event: &crate::models::CalendarEvent,
    event_id: &str,
    props: &Rules,
    missing_keys: &[&String],
) -> Result<()> {
    let mut new_props = event.shared_properties();
    for key in missing_keys {
        if let Some(key_values) = props.get(*key) {
            let key_prompt = format!("Select {key}:");
            if let Some(value) = prompt_select(&key_prompt, key_values)? {
                new_props.insert((*key).clone(), value);
            }
        }
    }
    client
        .patch_event_properties(calendar_id, event_id, &new_props)
        .await?;
    println!("fixed.");
    Ok(())
}
