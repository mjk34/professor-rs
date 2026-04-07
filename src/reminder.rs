use crate::data;
use crate::helper::default_footer;
use crate::serenity;
use chrono::{Datelike, NaiveDate, TimeDelta, Utc};
use poise::serenity_prelude::{ChannelId, CreateMessage};
use std::env;
use std::fs::{write, File};
use std::io::{BufRead, BufReader};

const EVENT_FILE: &str = ".eventdb";

async fn import_from_file(filename: &str) -> Vec<Vec<String>> {
    let file = match File::open(filename) {
        Ok(f) => f,
        Err(_) => {
            tracing::warn!("Event file '{}' not found, starting with empty database", filename);
            return Vec::new();
        }
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
            let cells: Vec<String> = line.split(',').map(|cell| cell.to_string()).collect();
            if cells.len() == 5 { Some(cells) } else { None }
        })
        .collect()
}

async fn export_to_file(filename: &str, database: Vec<Vec<String>>) {
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
    let mut database: Vec<Vec<String>> = import_from_file(EVENT_FILE).await;
    let length = database.len();
    let tz_offset: i64 = env::var("TZ_OFFSET_HOURS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(-4);
    let now = Utc::now() - TimeDelta::hours(tz_offset);
    let today = now.date_naive();
    let year = today.year();

    for row in database.iter_mut().take(length) {
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

        if (0..=14).contains(&days_until) && !reminded {
            let mod_chat: u64 = env::var("MOD_CHAT")
                .expect("Failed to load MODERATOR chat id")
                .parse()
                .unwrap();

            let mod_id: u64 = env::var("MOD_ID")
                .expect("Failed to load MODERATOR ping id")
                .parse()
                .unwrap();

            let desc = format!(
                "Hey <@&{}>, {}'s Birthday is coming up in 2 weeks! ({})",
                mod_id, name, next_occurrence
            );

            row[3] = "1".to_string();

            ChannelId::new(mod_chat)
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
                .unwrap();
        }

        if today == date_this_year && !pinged {
            let desc = format!(
                "Heyyyyyyy, its someone's special day!! It's {}'s (<@{}>) Birthday!!!",
                name, user_id
            );

            let gen_chat: u64 = env::var("GENERAL")
                .expect("Failed to load GENERAL chat id")
                .parse()
                .unwrap();

            row[4] = "1".to_string();

            ChannelId::new(gen_chat)
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
                .unwrap();
        }

    }

    export_to_file(EVENT_FILE, database).await;
}
