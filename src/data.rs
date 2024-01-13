use crate::serenity;
use chrono::prelude::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serenity::Color;
use std::collections::HashMap;
use std::fs;
use std::ops::Index;
use tokio::sync::Mutex;

// General Structures
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct ClipData {
    id: usize,
    title: String,
    link: String,
    date: DateTime<Utc>,
    rating: i32,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct WishData {
    small_pity: i32,
    big_pity: i32,
    wishes: i32,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct ItemData {
    name: String,
    desc: String,
    effect: i32,
    cost: i32,
}

// Event Structures
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct PokeData {
    name: String,
    desc: String,
    nickname: String,
    sprite: String,
    health: i32,
    types: (String, Option<String>),
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct EventData {
    name: String,
    buddy: i32,
    team: Vec<PokeData>,
    store: Vec<ItemData>,
}

// User profile
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct UserData {
    level: i32,
    xp: i32,

    name: String,
    creds: i32,
    last_daily: DateTime<Utc>,
    claimed_bonus: DateTime<Utc>,

    submits: Vec<ClipData>,
    wish: WishData,

    event: EventData,
    bag: Vec<ItemData>,
}

impl UserData {
    pub fn update_level(&mut self) {
        self.level = self.level + 1;
    }
    pub fn update_xp(&mut self, xp: i32) -> bool {
        if xp < 0 {
            return false;
        }

        self.xp = xp;
        return true;
    }

    pub fn update_name(&mut self, name: String) -> bool {
        if name == "" {
            return false;
        }

        self.name = name;
        return true;
    }
    pub fn update_daily(&mut self) {
        self.last_daily = Utc::now();
    }
    pub fn update_claimed_bonus(&mut self) {
        self.claimed_bonus = Utc::now();
    }

    pub fn add_creds(&mut self, creds: i32) -> bool {
        if creds < 0 {
            return false;
        }

        self.creds = creds;
        return true;
    }
    pub fn sub_creds(&mut self, creds: i32) -> bool {
        if creds > 0 {
            return false;
        }

        self.creds = -creds;
        return true;
    }

    pub fn add_submit(&mut self, new_submit: ClipData) {
        self.submits.push(new_submit);
    }
    pub fn get_submit_index(&self, clip_id: usize) -> Option<usize> {
        // cycles through self.submits, get the index
        // associated with the clip id
        if self.submits.len() <= 0 {
            return None;
        }

        for i in 0..self.submits.len() {
            if self.submits[i].id == clip_id {
                return Some(i);
            }
        }

        return None;
    }
    pub fn remove_submit(&mut self, submit_index: usize) -> bool {
        if submit_index >= self.submits.len() {
            return false;
        }
        if submit_index < 0 {
            return false;
        }
        if self.submits.len() <= 0 {
            return false;
        }

        self.submits.remove(submit_index);
        return true;
    }
    pub fn get_submissions(&self) -> Option<Vec<String>> {
        let mut submissions: Vec<String> = vec![];
        let mut counter = 0;

        for clip in &self.submits {
            let clip_string = format!("{} - {} {}", clip.id, clip.date.date_naive(), clip.title);
            submissions.push(clip_string);
            counter += 1;
        }

        return match counter {
            0 => None,
            _ => Some(submissions),
        };
    }

    pub fn update_small_pity(&mut self, small_pity: i32) -> bool {
        if small_pity < 0 {
            return false;
        }

        self.wish.small_pity = small_pity;
        return true;
    }
    pub fn update_big_pity(&mut self, big_pity: i32) -> bool {
        if big_pity < 0 {
            return false;
        }

        self.wish.big_pity = big_pity;
        return true;
    }
    pub fn update_wishes(&mut self, wish_count: i32) -> bool {
        if wish_count < 0 {
            return false;
        }

        self.wish.wishes = wish_count;
        return true;
    }
}

#[derive(Default)]
pub struct Data {
    /// User data, which is stored and accessible in all command invocations
    pub users: Mutex<HashMap<serenity::UserId, UserData>>,
}
impl Data {
    pub async fn check_or_create_user<'a>(
        &self,
        ctx: crate::Context<'a>,
    ) -> Result<(), crate::Error> {
        let user_id = ctx.author().id;
        {
            let mut data = self.users.lock().await;
            if data.contains_key(&user_id) {
                return Ok(());
            }

            data.insert(user_id, Default::default());
        }
        self.save().await;
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Account Created!")
                    .description(format!("{}", ctx.author().name))
                    .image(
                        "https://gifdb.com/images/high/anime-girl-okay-sign-b5zlye5h8mnjhdg2.gif",
                    )
                    .thumbnail(ctx.author().avatar_url().unwrap())
                    .color(Color::GOLD),
            ),
        )
        .await?;

        Ok(())
    }
    /// Attempts to save the data to a file
    ///
    /// Make sure that the Mutex is unlocked before calling this function
    pub async fn save(&self) {
        let users = self.users.lock().await;
        let encoded = serde_json::to_string(&users.clone()).unwrap();
        fs::write("data.json", encoded).expect("Failed to write binary save file");
    }

    /// Attempts to load the Data from a file, otherwise return a default
    pub fn load() -> Data {
        let data = fs::read_to_string("data.json").ok();
        let users: HashMap<serenity::UserId, UserData> = if let Some(file) = data {
            serde_json::from_str(&file).expect("Old data format?")
        } else {
            HashMap::default()
        };
        return Data {
            users: Mutex::new(users),
        };
    }
}
