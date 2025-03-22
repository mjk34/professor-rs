use crate::serenity;
use chrono::prelude::{DateTime, Utc};
use dashmap::DashMap;
use poise::serenity_prelude::RoleId;
use serde::{Deserialize, Serialize};
use serenity::Color;
use std::sync::Arc;
use std::{env, fs};
use tokio::sync::RwLock;

// Constants
pub const NUMBER_EMOJS: [&str; 10] = [
    "\u{0030}\u{FE0F}\u{20E3}",
    "\u{0031}\u{FE0F}\u{20E3}",
    "\u{0032}\u{FE0F}\u{20E3}",
    "\u{0033}\u{FE0F}\u{20E3}",
    "\u{0034}\u{FE0F}\u{20E3}",
    "\u{0035}\u{FE0F}\u{20E3}",
    "\u{0036}\u{FE0F}\u{20E3}",
    "\u{0037}\u{FE0F}\u{20E3}",
    "\u{0038}\u{FE0F}\u{20E3}",
    "\u{0039}\u{FE0F}\u{20E3}",
];

pub const EMBED_DEFAULT: Color = Color::new(16119285); // white - transition color
pub const EMBED_CYAN: Color = Color::new(6943230); // cyan  - good finish color
pub const EMBED_GOLD: Color = Color::GOLD; // gold - cred related color
pub const EMBED_FAIL: Color = Color::RED; // red - absolute fails
pub const EMBED_LEVEL: Color = Color::ORANGE; // orange - level/xp related color
pub const EMBED_SUCCESS: Color = Color::new(65280); // green - major success
pub const EMBED_ERROR: Color = Color::new(6053215); // grey - soft fails
pub const EMBED_MOD: Color = Color::new(16749300); // pink - moderator commands

// General Structures
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct ClipData {
    pub title: String,
    pub link: String,
    pub date: DateTime<Utc>,
    pub rating: Option<f64>,
}

impl ClipData {
    pub fn new(title: String, link: String) -> Self {
        ClipData {
            title,
            link,
            date: Utc::now(),
            rating: None,
        }
    }
}

// User profile
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct UserData {
    level: i32,
    xp: i32,

    creds: i32,
    rolls: i32,
    daily_count: i32,
    bonus_count: i32,
    last_daily: DateTime<Utc>,

    pub submits: Vec<Option<ClipData>>,
    tickets: i32,
}

impl UserData {
    pub fn update_level(&mut self) {
        self.level += 1;
    }

    pub fn update_xp(&mut self, xp: i32) -> bool {
        if xp < 0 {
            return false;
        }

        self.xp += xp;
        let xp_cap = 500 + self.get_level() * 80;

        if self.xp >= xp_cap {
            self.xp -= xp_cap;
            self.update_level();

            return true;
        }
        false
    }

    pub fn update_daily(&mut self) {
        self.last_daily = Utc::now();
        self.daily_count += 1;
    }

    pub fn add_rolls(&mut self, roll: i32) -> bool {
        if roll < 1 {
            return false;
        }

        self.rolls += roll;
        true
    }

    pub fn check_daily(&self) -> bool {
        let diff = Utc::now() - self.last_daily;
        diff.num_hours() >= 21
    }

    pub fn add_bonus(&mut self) {
        if self.bonus_count == 3 {
            self.bonus_count = 3;
        } else {
            self.bonus_count += 1;
        }
    }

    pub fn reset_bonus(&mut self) {
        self.bonus_count = 0;
    }

    pub fn check_claim(&self) -> bool {
        matches!(self.bonus_count, 3)
    }

    pub fn add_creds(&mut self, creds: i32) -> bool {
        if creds < 0 {
            return false;
        }

        self.creds += creds;
        true
    }

    pub fn sub_creds(&mut self, creds: i32) -> bool {
        if creds < 0 {
            return false;
        }
        self.creds -= creds;
        true
    }

    pub fn add_tickets(&mut self, tickets: i32) -> bool {
        if tickets < 1 {
            return false;
        }

        self.tickets += tickets;
        true
    }

    pub fn get_creds(&self) -> i32 {
        self.creds
    }

    pub fn get_tickets(&self) -> i32 {
        self.tickets
    }

    pub fn get_luck(&self) -> String {
        if self.daily_count == 0 {
            return "N/A".to_string();
        }

        let average = self.get_luck_score();
        let luck: String;
        if average < 6 {
            luck = "Horrible".to_string();
        } else if (6..9).contains(&average) {
            luck = "Below Average".to_string();
        } else if (9..12).contains(&average) {
            luck = "Average".to_string();
        } else if (12..15).contains(&average) {
            luck = "Above Average".to_string();
        } else {
            luck = "Blessed".to_string();
        }

        luck
    }

    pub fn get_luck_score(&self) -> i32 {
        self.rolls / (self.daily_count + 1)
    }

    pub fn get_bonus(&self) -> i32 {
        self.bonus_count
    }

    pub fn get_level(&self) -> i32 {
        self.level
    }

    pub fn get_xp(&self) -> i32 {
        self.xp
    }

    pub fn get_next_level(&self) -> i32 {
        500 + self.get_level() * 80
    }

    pub fn add_submit(&mut self, new_submit: ClipData) -> bool {
        for i in 0..5 {
            let s = self.submits.get_mut(i);
            if let Some(s) = s {
                if s.is_none() {
                    *s = Some(new_submit);
                    return true;
                }
            } else {
                self.submits.push(Some(new_submit));
                return true;
            }
        }
        false
    }

    pub fn remove_submit(&mut self, submit_index: usize) -> bool {
        let res = self.submits.remove(submit_index);
        res.is_some()
    }

    pub fn get_submissions(&self, show_score: bool, show_icon: bool) -> Vec<String> {
        let mut submissions: Vec<String> = vec![];
        for (id, clip) in self.submits.iter().enumerate() {
            if let Some(clip) = clip {
                let score = if let Some(s) = clip.rating {
                    format!("[{}/5]", s)
                } else {
                    "[-/5]".to_string()
                };
                let clip_string = format!(
                    "{} {} **[{}]({})** ({})",
                    if show_icon { NUMBER_EMOJS[id] } else { "" },
                    if show_score {
                        format!(" {} ", score)
                    } else {
                        "".to_string()
                    },
                    clip.title,
                    clip.link,
                    clip.date.format("%m/%d")
                );
                submissions.push(clip_string);
            }
        }
        submissions
    }
}

#[derive(Debug, Clone)]
pub struct VoiceUser {
    pub joined: DateTime<Utc>,
    pub last_reward: Option<DateTime<Utc>>,
    pub mute: Option<DateTime<Utc>>,
    pub deaf: Option<DateTime<Utc>>,
}

impl VoiceUser {
    pub fn new() -> VoiceUser {
        VoiceUser {
            joined: Utc::now(),
            last_reward: None,
            mute: None,
            deaf: None,
        }
    }
    pub fn update_mute(&mut self, b: bool) {
        if b {
            self.mute = Some(Utc::now());
        } else {
            self.mute = None;
        }
    }
    pub fn update_deaf(&mut self, b: bool) {
        if b {
            self.deaf = Some(Utc::now());
        } else {
            self.deaf = None;
        }
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct SaveData {
    pub users: DashMap<serenity::UserId, UserData>,
}

impl std::ops::Deref for SaveData {
    type Target = DashMap<serenity::UserId, UserData>;

    fn deref(&self) -> &Self::Target {
        &self.users
    }
}

/// User data, which is stored and accessible in all command invocations
#[derive(Default)]
pub struct Data {
    /// Persistent data of users
    pub users: Arc<DashMap<serenity::UserId, Arc<RwLock<UserData>>>>,
    /// Duration of users in voice channel, updates by events
    pub voice_users: Arc<DashMap<serenity::UserId, VoiceUser>>,
    pub meme: Vec<String>,
    pub ponder: Vec<String>,
    pub pong: Vec<String>,
    pub d20f: Vec<String>,
    pub mod_id: RoleId,
}

impl Data {
    pub async fn check_or_create_user(ctx: crate::Context<'_>) -> Result<(), crate::Error> {
        let user_id = ctx.author().id;
        {
            let data = &ctx.data().users;
            // let data = &mut ctx.data().users;
            if data.contains_key(&user_id) {
                return Ok(());
            }

            data.insert(user_id, Default::default());
        }

        ctx.send(
            poise::CreateReply::default()
            .content(format!("<@{}>", ctx.author().id))
            .embed(
                serenity::CreateEmbed::new()
                    .title("Account Created!")
                    .description(format!("Welcome <@{}>! You are now registered with ProfessorBot, feel free to checkout Professors Commands in https://discord.com/channels/859993171156140061/860013281165967380", ctx.author().id))
                    .image(
                        "https://cdn.discordapp.com/attachments/1260223476766343188/1262191655763578881/anime-girl-okay-sign-b5zlye5h8mnjhdg2.gif?ex=6695b315&is=66946195&hm=215e00c0ee066c4a36a8c837f7b24570d2736dae19713e220702114330667f6c&",
                    )
                    .color(EMBED_DEFAULT),
            ),
        )
        .await?;

        Ok(())
    }

    /// Attempts to save the data to a file
    pub async fn save(&self) {
        let users = Arc::clone(&self.users);
        let users_save = DashMap::new();

        for x in users.iter() {
            let (id, u) = x.pair();
            let u = u.read().await;
            users_save.insert(*id, u.clone());
        }

        let users_save = SaveData { users: users_save };

        let encoded = serde_json::to_string(&users_save).unwrap();
        fs::write("data.json", encoded).expect("Failed to write binary save file");
    }

    /// Attempts to load the Data from a file, otherwise return a default
    pub fn load() -> Data {
        let data = fs::read_to_string("data.json").ok();
        let users_data: SaveData = if let Some(file) = data {
            serde_json::from_str(&file).expect("Old data format?")
        } else {
            SaveData::default()
        };

        let users = Arc::new(DashMap::default());
        for x in users_data.iter() {
            let (id, u) = x.pair();
            users.insert(*id, Arc::new(RwLock::new(u.clone())));
        }

        let meme = read_lines("reference/meme.txt");
        let ponder = read_lines("reference/ponder.txt");
        let pong = read_lines("reference/pong.txt");
        let d20f = read_lines("reference/d20.txt");

        let mod_id = RoleId::new(
            env::var("MOD_ID")
                .expect("Missing moderator ID")
                .parse()
                .unwrap(),
        );

        Data {
            users,
            voice_users: Arc::new(DashMap::new()),
            meme,
            ponder,
            pong,
            d20f,
            mod_id,
        }
    }
}

fn read_lines(filename: &str) -> Vec<String> {
    let lines: Vec<String> = fs::read_to_string(filename)
        .unwrap()
        .lines()
        .map(String::from)
        .collect();

    // println!("{}: loaded {} lines", filename, lines.len());
    lines
}
