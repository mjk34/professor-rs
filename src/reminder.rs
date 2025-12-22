use crate::data;
use crate::serenity;
use chrono::{Datelike, NaiveDate, TimeDelta, Utc};
use poise::serenity_prelude::{ChannelId, CreateMessage};
use std::env;
use std::fs::{write, File};
use std::io::{BufRead, BufReader};

const EVENT_FILE: &str = ".eventdb";

fn import_from_file(filename: &str) -> Vec<Vec<String>> {
    let mut file_descriptor = BufReader::new(File::open(filename).unwrap());
    let mut buffer = String::new();

    file_descriptor.read_line(&mut buffer).unwrap();
    file_descriptor
        .lines()
        .map(|line| {
            line.unwrap()
                .split(',')
                .map(|cell| cell.parse().unwrap())
                .collect()
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
    let length = database.len();
    let now = Utc::now() - TimeDelta::hours(4);
    let today = now.date_naive();
    let year = today.year();

    //TODO: finish this shit
    for i in 0..length {
        let mut date_parts = database[i][0].split('-');
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
        let name: String = database[i][1].clone();
        let user_id: String = database[i][2].clone();
        let mut reminded: bool = matches!(database[i][3].as_str(), "1");
        let mut pinged: bool = matches!(database[i][4].as_str(), "1");

        if today > date_this_year && (reminded || pinged) {
            database[i][3] = "0".to_string();
            database[i][4] = "0".to_string();
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

            database[i][3] = "1".to_string();

            ChannelId::new(mod_chat)
                .send_message(
                    http,
                    CreateMessage::default().embed(
                        serenity::CreateEmbed::default()
                            .title("Reminder")
                            .description(&desc)
                            .color(data::EMBED_CYAN)
                            .footer(serenity::CreateEmbedFooter::new(
                                "@~ powered by UwUntu & RustyBamboo",
                            )),
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

            database[i][4] = "1".to_string();

            ChannelId::new(gen_chat)
                .send_message(
                    http,
                    CreateMessage::default().embed(
                        serenity::CreateEmbed::default()
                            .title("Reminder")
                            .description(&desc)
                            .color(data::EMBED_CYAN)
                            .footer(serenity::CreateEmbedFooter::new(
                                "@~ powered by UwUntu & RustyBamboo",
                            )),
                    ),
                )
                .await
                .unwrap();
        }

    }

    export_to_file(EVENT_FILE, database);
}
