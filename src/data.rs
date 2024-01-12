use crate::serenity;
use chrono::prelude::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::fs;
use tokio::sync::Mutex;

// General Structures
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct ClipData {
    id: String,
    title: String,
    link: String,
    date: DateTime<Utc>,
    rating: i32,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct WishData {
    small_pity: i32,
    big_pity: i32,
    wishes:i32,
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
    bag: Vec<ItemData>
}

impl UserData {
    pub fn update_level(&mut self, level: i32) {
        self.level = level;
    }
    pub fn update_xp(&mut self, xp: i32){
        self.xp = xp;
    }

    pub fn update_name(&mut self, name: String) {
        self.name = name;
    }
    pub fn update_daily(&mut self) {
        self.last_daily = Utc::now();
    }
    pub fn update_creds(&mut self, creds: i32){
        self.creds = creds;
    }
    pub fn update_claimed_bonus(&mut self){
        self.claimed_bonus = Utc::now();
    }

    pub fn add_submit(&mut self, new_submit: ClipData){
        self.submits.push(new_submit);
    }
    pub fn get_submit_index(&self, clip_id:i32){
        // cycles through self.submits, get the index
        // associated with the clip id
        let index: i32;
    }
    pub fn remove_submit(&mut self, submit_index: i32){
        self.submits.remove(submit_index);
    }

    pub fn update_small_pity(&mut self, small_pity:i32){
        self.wish.small_pity = small_pity;
    }
    pub fn update_big_pity(&mut self, big_pity:i32){
        self.wish.big_pity = big_pity;
    }
    pub fn update_wishes(&mut self, wish_count:i32){
        self.wish.wishes = wish_count;
    }


    
}

#[derive(Default)]
pub struct Data {
    pub users: Mutex<HashMap<serenity::UserId, UserData>>,
} // User data, which is stored and accessible in all command invocations
impl Data {
    pub async fn save(&self) {
        let users = self.users.lock().await;
        let encoded: Vec<u8> = bincode::serialize(&users.clone()).unwrap();
        fs::write("data.bin", encoded).expect("Could write to binary save file");
    }

    // pub fn load() {
    //     let data = fs::read("data.bin").ok();
    //     let users = if let Some(file) = data {
    //         bincode::deserialize::<HashMap<serenity::UserId, UserData>>(&file).unwrap();
    //     } else {
    //         Data::default();
    //     };
    
    // }
}
