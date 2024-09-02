//!---------------------------------------------------------------------!
//! This file contains a collection of internal functions to help       !
//! reduce repetitive code                                              !
//!                                                                     !
//! Commands:                                                           !
//!     [ ] - parse_user_mention                                        !
//!---------------------------------------------------------------------!

use chrono::{Datelike, NaiveDate, TimeDelta, Utc};
use poise::serenity_prelude::UserId;

pub fn parse_user_mention(user_mention: String) -> u64 {
    user_mention
        .replace(&['<', '>', '!', '@', '&'][..], "")
        .parse::<u64>()
        .unwrap_or(1)
}

pub fn get_current_date() -> String {
    let today = Utc::now() - TimeDelta::hours(4);
    format!("{}-{}-{}", today.year(), today.month(), today.day())
}

pub fn get_reminder_date(date_str: &str) -> String {
    let date_parsed = NaiveDate::parse_from_str(date_str, "%Y-%m-%d").unwrap();
    let reminder = date_parsed - TimeDelta::days(14);
    format!(
        "{}-{}-{}",
        reminder.year(),
        reminder.month(),
        reminder.day()
    )
}

pub fn get_current_year() -> String {
    let today = Utc::now() - TimeDelta::hours(4);
    format!("{}", today.year())
}

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
        for (index, (_id, creds, _, user_name)) in info.iter().enumerate().skip(start).take(10) {
            let content = if index == 0 {
                format!(
                    "\u{3000}** #{} ** \u{3000}\u{3000} **{}** \u{3000}\u{3000}{}\n",
                    index + 1,
                    user_name,
                    creds
                )
            } else if index > 9 {
                format!(
                    "\u{3000}** #{} ** \u{3000}\u{2000} {} \u{3000}\u{3000}{}\n",
                    index + 1,
                    user_name,
                    creds
                )
            } else {
                format!(
                    "\u{3000}** #{} ** \u{3000}\u{3000} {} \u{3000}\u{3000}{}\n",
                    index + 1,
                    user_name,
                    creds
                )
            };

            leaderboard_text.push_str(&content);
        }
    }

    if sort == fortune {
        for (index, (_id, _, luck, user_name)) in info.iter().enumerate().skip(start).take(10) {
            println!("test luck {}", luck);
            let content = if index == 0 {
                format!(
                    "\u{3000}** #{} ** \u{3000}\u{3000} **{}** \u{3000}\u{3000}{}\n",
                    index + 1,
                    user_name,
                    luck
                )
            } else if index > 9 {
                format!(
                    "\u{3000}** #{} ** \u{3000}\u{2000} {} \u{3000}\u{3000}{}\n",
                    index + 1,
                    user_name,
                    luck
                )
            } else {
                format!(
                    "\u{3000}** #{} ** \u{3000}\u{3000} {} \u{3000}\u{3000}{}\n",
                    index + 1,
                    user_name,
                    luck
                )
            };

            leaderboard_text.push_str(&content);
        }
    }

    if sort == level {
        for (index, (_id, _, level, user_name)) in info.iter().enumerate().skip(start).take(10) {
            let content = if index == 0 {
                format!(
                    "\u{3000}** #{} ** \u{3000}\u{3000} **{}** \u{3000}\u{3000}{}\n",
                    index + 1,
                    user_name,
                    level
                )
            } else if index > 9 {
                format!(
                    "\u{3000}** #{} ** \u{3000}\u{2000} {} \u{3000}\u{3000}{}\n",
                    index + 1,
                    user_name,
                    level
                )
            } else {
                format!(
                    "\u{3000}** #{} ** \u{3000}\u{3000} {} \u{3000}\u{3000}{}\n",
                    index + 1,
                    user_name,
                    level
                )
            };

            leaderboard_text.push_str(&content);
        }
    }

    leaderboard_text
}
