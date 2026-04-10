//! Birthday and event reminders: reads/writes `.eventdb` and fires Discord messages.
use crate::data;
use crate::helper::default_footer;
use crate::serenity;
use chrono::{Datelike, NaiveDate, TimeDelta, Utc};
use poise::serenity_prelude::{ChannelId, CreateMessage};
use std::env;
use std::fs::{write, File};
use std::io::{BufRead, BufReader};

/// Path to the flat-file birthday/event database (CSV, one entry per line).
const EVENT_FILE: &str = ".eventdb";
/// Number of days in advance to send a birthday reminder to the mod channel.
const BIRTHDAY_REMINDER_DAYS_AHEAD: i64 = 14;
/// Default timezone offset from UTC when `TZ_OFFSET_HOURS` is unset — EDT (UTC-4).
const DEFAULT_TZ_OFFSET_HOURS: i64 = -4;

fn import_from_file(filename: &str) -> Vec<Vec<String>> {
    let file = if let Ok(f) = File::open(filename) { f } else {
        tracing::warn!(file = %filename, "event file not found — starting with empty database");
        return Vec::new();
    };

    let mut file_descriptor = BufReader::new(file);
    let mut buffer = String::new();

    if file_descriptor.read_line(&mut buffer).is_err() {
        return Vec::new();
    }

    file_descriptor
        .lines()
        .filter_map(|line| {
            let line = line.ok()?;
            let cells: Vec<String> = line.split(',').map(std::string::ToString::to_string).collect();
            if cells.len() == 5 { Some(cells) } else { None }
        })
        .collect()
}

fn export_to_file(filename: &str, database: Vec<Vec<String>>) {
    let mut contents = String::new();
    contents += "-\n";
    for line in database {
        contents += format!(
            "{},{},{},{},{}\n",
            line[0], line[1], line[2], line[3], line[4]
        )
        .as_str();
    }

    contents.pop();

    let _ = write(filename, contents);
}

pub async fn check_birthday(http: &serenity::Http) {
    let mut database: Vec<Vec<String>> = import_from_file(EVENT_FILE);
    let tz_offset: i64 = env::var("TZ_OFFSET_HOURS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_TZ_OFFSET_HOURS);
    let now = Utc::now() - TimeDelta::hours(tz_offset);
    let today = now.date_naive();
    let year = today.year();

    for row in &mut database {
        let mut date_parts = row[0].split('-');
        let month = match date_parts.next().and_then(|part| part.parse::<u32>().ok()) {
            Some(month) => month,
            None => continue,
        };
        let day = match date_parts.next().and_then(|part| part.parse::<u32>().ok()) {
            Some(day) => day,
            None => continue,
        };
        let date_this_year = match NaiveDate::from_ymd_opt(year, month, day) {
            Some(date) => date,
            None => continue,
        };
        let date_next_year = NaiveDate::from_ymd_opt(year + 1, month, day);
        let name: String = row[1].clone();
        let user_id: String = row[2].clone();
        let mut reminded: bool = matches!(row[3].as_str(), "1");
        let mut pinged: bool = matches!(row[4].as_str(), "1");

        if today > date_this_year && (reminded || pinged) {
            row[3] = "0".to_string();
            row[4] = "0".to_string();
            reminded = false;
            pinged = false;
        }

        let next_occurrence = if today <= date_this_year {
            date_this_year
        } else if let Some(date) = date_next_year {
            date
        } else {
            continue;
        };

        let days_until = (next_occurrence - today).num_days();

        if (0..=BIRTHDAY_REMINDER_DAYS_AHEAD).contains(&days_until) && !reminded {
            let Ok(mod_chat) = env::var("MOD_CHAT").unwrap_or_default().parse::<u64>() else {
                tracing::warn!("reminder: MOD_CHAT unset or invalid — skipping birthday reminder");
                continue;
            };
            let Ok(mod_id) = env::var("MOD_ID").unwrap_or_default().parse::<u64>() else {
                tracing::warn!("reminder: MOD_ID unset or invalid — skipping birthday reminder");
                continue;
            };

            let desc = format!(
                "Hey <@&{mod_id}>, {name}'s Birthday is coming up in 2 weeks! ({next_occurrence})"
            );

            row[3] = "1".to_string();

            if let Err(e) = ChannelId::new(mod_chat)
                .send_message(
                    http,
                    CreateMessage::default().embed(
                        serenity::CreateEmbed::default()
                            .title("Reminder")
                            .description(&desc)
                            .color(data::EMBED_CYAN)
                            .footer(default_footer()),
                    ),
                )
                .await
            {
                tracing::warn!(error = %e, "failed to send birthday reminder to mod channel");
            }
        }

        if today == date_this_year && !pinged {
            let desc = format!(
                "Heyyyyyyy, its someone's special day!! It's {name}'s (<@{user_id}>) Birthday!!!"
            );

            let Ok(gen_chat) = env::var("GENERAL").unwrap_or_default().parse::<u64>() else {
                tracing::warn!("reminder: GENERAL unset or invalid — skipping birthday ping");
                continue;
            };

            row[4] = "1".to_string();

            if let Err(e) = ChannelId::new(gen_chat)
                .send_message(
                    http,
                    CreateMessage::default().embed(
                        serenity::CreateEmbed::default()
                            .title("Reminder")
                            .description(&desc)
                            .color(data::EMBED_CYAN)
                            .footer(default_footer()),
                    ),
                )
                .await
            {
                tracing::warn!(error = %e, "failed to send birthday ping to general channel");
            }
        }

    }

    export_to_file(EVENT_FILE, database);
}
