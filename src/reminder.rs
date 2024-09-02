use crate::data;
use crate::helper::{get_current_date, get_reminder_date};
use crate::{serenity, Context};
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

pub async fn check_birthday(ctx: Context<'_>) {
    let mut database: Vec<Vec<String>> = import_from_file(EVENT_FILE);
    let length = database.len();
    let today: String = get_current_date();

    //TODO: finish this shit
    for i in 0..length {
        let date: String = format!("2024-{}", database[i][0]);
        let name: String = database[i][1].clone();
        let user_id: String = database[i][2].clone();
        let reminded: bool = matches!(database[i][3].as_str(), "1");
        let pinged: bool = matches!(database[i][4].as_str(), "1");

        let reminder: String = get_reminder_date(&date);

        let mut desc = String::new();

        if today == reminder && !reminded {
            let mod_chat: u64 = env::var("MOD_CHAT")
                .expect("Failed to load MODERATOR chat id")
                .parse()
                .unwrap();

            let mod_id: u64 = env::var("MOD_ID")
                .expect("Failed to load MODERATOR chat id")
                .parse()
                .unwrap();

            desc = format!(
                "Hey <@&{}>, {}'s Birthday is coming up in 2 weeks! ({})",
                mod_id, name, date
            );

            database[i][3] = "1".to_string();

            ChannelId::new(mod_chat)
                .send_message(
                    ctx.http(),
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

        if today == date && !pinged {
            desc = format!(
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
                    ctx.http(),
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

        if desc == String::new() {}
    }

    export_to_file(EVENT_FILE, database);

    ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::default()
                .title("Reminder")
                .description("Command - success")
                .color(data::EMBED_CYAN)
                .footer(serenity::CreateEmbedFooter::new(
                    "@~ powered by UwUntu & RustyBamboo",
                )),
        ),
    )
    .await
    .unwrap();
}
