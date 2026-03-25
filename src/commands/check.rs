use anyhow::{Context, Result, bail};

use crate::cli::{CheckArgs, OutputOptions};
use crate::client::GoogleCalendarClient;
use crate::config::Config;
use crate::fetch_events;
use crate::helpers::{prompt_select, prompt_yes_no_quit};

pub async fn cmd_check(client: &GoogleCalendarClient, args: &CheckArgs, config: &Config, out: &OutputOptions) -> Result<()> {
    let check_rules = config.check.as_ref()
        .context("no [check] section in config.toml")?;
    if check_rules.is_empty() {
        bail!("no rules defined in [check] section of config.toml");
    }

    let properties = if args.fix {
        Some(config.properties.as_ref()
            .context("--fix requires [properties] section in config.toml")?)
    } else {
        None
    };

    let (calendar_id, events) = fetch_events(client, args.calendar_name.as_deref(), config).await?;
    if events.is_empty() {
        if !out.quiet { println!("No events found."); }
        return Ok(());
    }

    let mut issues = 0u32;
    let mut events_with_issues = 0u32;
    let mut fixed = 0u32;
    for event in &events {
        let summary = event.summary_or_default();
        let start = event.start_str();
        let shared = event.extended_properties.as_ref().and_then(|p| p.shared.as_ref());
        let event_id = event.id.as_deref();

        let event_type = match shared.and_then(|s| s.get("type")) {
            Some(t) => t.clone(),
            None => {
                println!("{summary} ({start}):");
                println!("  - missing 'type' property");
                issues += 1;
                events_with_issues += 1;

                if args.fix {
                    if let (Some(props), Some(eid)) = (&properties, event_id) {
                        if let Some(type_values) = props.get("type") {
                            let prompt = format!("  Fix: select type for '{summary}':");
                            if let Some(chosen_type) = prompt_select(&prompt, type_values)? {
                                let mut new_props = event.shared_properties();
                                new_props.insert("type".to_string(), chosen_type.clone());

                                // Now check required properties for the chosen type
                                if let Some(required) = check_rules.get(&chosen_type) {
                                    for key in required {
                                        if !new_props.contains_key(key) {
                                            if let Some(key_values) = props.get(key) {
                                                let key_prompt = format!("  Fix: select {key}:");
                                                if let Some(value) = prompt_select(&key_prompt, key_values)? {
                                                    new_props.insert(key.clone(), value);
                                                }
                                            }
                                        }
                                    }
                                }

                                client.patch_event_properties(&calendar_id, eid, &new_props).await?;
                                println!("  fixed.");
                                fixed += 1;
                            }
                        }
                    }
                }
                continue;
            }
        };

        let required = match check_rules.get(&event_type) {
            Some(r) => r,
            None => {
                println!("{summary} ({start}):");
                println!("  - unknown type '{event_type}' (not in [check] config)");
                issues += 1;
                events_with_issues += 1;
                continue;
            }
        };

        let mut missing_keys: Vec<&String> = Vec::new();
        for key in required {
            if shared.and_then(|s| s.get(key)).is_none() {
                missing_keys.push(key);
            }
        }

        if !missing_keys.is_empty() {
            println!("{summary} ({start}):");
            for key in &missing_keys {
                println!("  - missing required property '{key}' (required for type '{event_type}')");
            }
            issues += missing_keys.len() as u32;
            events_with_issues += 1;

            if args.fix {
                if let (Some(props), Some(eid)) = (&properties, event_id) {
                    let prompt = format!("  Fix this event?");
                    match prompt_yes_no_quit(&prompt)? {
                        Some(true) => {
                            let mut new_props = event.shared_properties();
                            for key in &missing_keys {
                                if let Some(key_values) = props.get(*key) {
                                    let key_prompt = format!("  Select {key}:");
                                    if let Some(value) = prompt_select(&key_prompt, key_values)? {
                                        new_props.insert((*key).clone(), value);
                                    }
                                }
                            }
                            client.patch_event_properties(&calendar_id, eid, &new_props).await?;
                            println!("  fixed.");
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
    }

    if !out.quiet {
        if issues == 0 {
            println!("All {} event(s) pass checks.", events.len());
        } else {
            println!("\n{issues} issue(s) in {events_with_issues} event(s) out of {}.", events.len());
            if args.fix {
                println!("{fixed} event(s) fixed.");
            }
        }
    }

    Ok(())
}
