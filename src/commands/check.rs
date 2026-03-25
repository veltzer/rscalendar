use anyhow::{Context, Result, bail};

use crate::cli::{CheckArgs, OutputOptions};
use crate::client::GoogleCalendarClient;
use crate::config::Config;
use crate::fetch_events;

pub async fn cmd_check(client: &GoogleCalendarClient, args: &CheckArgs, config: &Config, out: &OutputOptions) -> Result<()> {
    let check_rules = config.check.as_ref()
        .context("no [check] section in config.toml")?;
    if check_rules.is_empty() {
        bail!("no rules defined in [check] section of config.toml");
    }

    let (_, events) = fetch_events(client, args.calendar_name.as_deref(), config).await?;
    if events.is_empty() {
        if !out.quiet { println!("No events found."); }
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

    if !out.quiet {
        if issues == 0 {
            println!("All {} event(s) pass checks.", events.len());
        } else {
            println!("\n{issues} issue(s) in {events_with_issues} event(s) out of {}.", events.len());
        }
    }

    Ok(())
}
