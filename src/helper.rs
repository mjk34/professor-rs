//!---------------------------------------------------------------------!
//! This file contains a collection of internal functions to help       !
//! reduce repetitive code                                              !
//!                                                                     !
//! Commands:                                                           !
//!     [ ] - parse_user_mention                                        !
//!---------------------------------------------------------------------!

use poise::serenity_prelude::UserId;

pub fn parse_user_mention(user_mention: &str) -> Option<u64> {
    user_mention
        .replace(&['<', '>', '!', '@', '&'][..], "")
        .parse::<u64>()
        .ok()
}

#[expect(dead_code)]
pub fn get_leaderboard(
    info: &[(UserId, i32, String, String)],
    sort: String,
    start: usize,
) -> String {
    let mut leaderboard_text = String::new();
    leaderboard_text.push_str("﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n");

    let creds: String = "Creds".to_string();
    let fortune: String = "Fortune".to_string();
    let level: String = "Level".to_string();

    if sort == creds {
        leaderboard_text.push_str("```\n"); // Start code block for monospaced font

        for (index, (_id, creds, _, user_name)) in info.iter().enumerate().skip(start).take(10) {
            let content = format!(
                "{:<3} | {:^20} | {:>15}\n",
                format!("#{}", index + 1), // Left-align index
                user_name,                 // Left-align user name
                creds                      // Left-align creds
            );
            leaderboard_text.push_str(&content);
        }
        leaderboard_text.push_str("```\n"); // End code block
    }

    if sort == fortune {
        leaderboard_text.push_str("```\n"); // Start code block for monospaced font

        for (index, (_id, _, luck, user_name)) in info.iter().enumerate().skip(start).take(10) {
            let content = format!(
                "{:<3} | {:^20} | {:>15}\n",
                format!("#{}", index + 1), // Left-align index
                user_name,                 // Left-align user name
                luck                       // Left-align creds
            );
            leaderboard_text.push_str(&content);
        }
        leaderboard_text.push_str("```\n"); // End code block
    }

    if sort == level {
        leaderboard_text.push_str("```\n"); // Start code block for monospaced font

        for (index, (_id, _, level, user_name)) in info.iter().enumerate().skip(start).take(10) {
            let content = format!(
                "{:<3} | {:^20} | {:>15}\n",
                format!("#{}", index + 1), // Left-align index
                user_name,                 // Left-align user name
                level                      // Left-align creds
            );
            leaderboard_text.push_str(&content);
        }
        leaderboard_text.push_str("```\n"); // End code block
    }

    leaderboard_text
}
