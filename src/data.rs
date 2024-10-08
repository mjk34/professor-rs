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
pub const EMBED_SUCCESS: Color = Color::new(65280); // green - major success
pub const EMBED_ERROR: Color = Color::new(6053215); // grey - soft fails
pub const EMBED_MOD: Color = Color::new(16749300); // pink - moderator commands
pub const EMBED_TRAINER: Color = Color::new(3756519); // blue - trainer battles

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

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct WishData {
    small_pity: i32,
    big_pity: i32,
    wishes: i32,
    guarentee: bool,
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
    index: usize,
    desc: String,
    nickname: Option<String>,
    sprite: String,
    bsprite: String,
    wallpaper: String,
    current_hp: Option<i32>,
    health: Option<i32>,
    types: String,
}

impl PokeData {
    pub fn set_current_health(&mut self, health: i32) {
        self.current_hp = Some(health);
    }

    pub fn set_health(&mut self, health: i32) {
        self.health = Some(health);
        self.current_hp = Some(health);
    }

    pub fn get_name(&self) -> String {
        self.name.clone()
    }

    pub fn get_index(&self) -> usize {
        self.index
    }

    pub fn get_desc(&self) -> String {
        self.desc.clone()
    }

    // pub fn get_nickname(&self) -> Option<String> {
    //     return self.nickname.clone();
    // }

    pub fn get_sprite(&self) -> String {
        self.sprite.clone()
    }

    pub fn get_bsprite(&self) -> String {
        self.bsprite.clone()
    }

    pub fn get_wallpaper(&self) -> String {
        self.wallpaper.clone()
    }

    pub fn get_health(&self) -> i32 {
        if self.health.is_none() {
            return 0;
        }

        self.health.unwrap()
    }

    pub fn get_current_health(&self) -> i32 {
        if self.current_hp.is_none() {
            return 0;
        }

        self.current_hp.unwrap()
    }

    pub fn get_types(&self) -> String {
        self.types.clone()
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct TrainerData {
    name: String,
    types: String,
    tier: String,
    team: Vec<PokeData>,
    wallpaper: String,
}

impl TrainerData {
    pub fn give_pokemon(&mut self, pokemon: PokeData) {
        self.team.push(pokemon);
    }

    pub fn get_name(&self) -> String {
        self.name.clone()
    }
    pub fn get_types(&self) -> String {
        self.types.clone()
    }
    pub fn get_tier(&self) -> String {
        self.tier.clone()
    }
    pub fn get_wallpaper(&self) -> String {
        self.wallpaper.clone()
    }
    pub fn get_team(&self) -> Vec<PokeData> {
        self.team.clone()
    }
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct EventData {
    name: String,
    buddy: usize,
    team: Vec<PokeData>,
    // add a personal pokedex that shows what you've caught
    store: Vec<ItemData>,
}

impl EventData {
    pub fn set_buddy(&mut self, buddy: usize) {
        self.buddy = buddy
    }

    pub fn add_pokemon(&mut self, pokemon: PokeData) {
        self.team.push(pokemon)
    }

    pub fn take_damage(&mut self, current: usize, damage: i32) {
        let health = self.team[current].get_current_health();
        self.team[current].set_current_health(health - damage);
    }

    pub fn faint_pokemon(&mut self, current: usize) {
        self.team[current].set_current_health(0);
    }

    pub fn get_buddy(&self) -> usize {
        self.buddy
    }

    pub fn get_team(&self) -> Vec<PokeData> {
        self.team.clone()
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
    wish: WishData,

    pub event: EventData,
    bag: Vec<ItemData>,
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
            self.add_wishes(3);

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
        diff.num_hours() >= 24
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

    pub fn add_wishes(&mut self, wishes: i32) -> bool {
        if wishes < 1 {
            return false;
        }

        self.wish.wishes = wishes;
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

    // pub fn get_submit_index(&self, clip_id: usize) -> Option<usize> {
    //     // cycles through self.submits, get the index
    //     // associated with the clip id
    //     if self.submits.len() <= 0 {
    //         return None;
    //     }
    //     for i in 0..self.submits.len() {
    //         if self.submits[i].id == clip_id {
    //             return Some(i);
    //         }
    //     }
    //     return None;
    // }

    pub fn remove_submit(&mut self, submit_index: usize) -> bool {
        let res = self.submits.remove(submit_index);
        res.is_some()
    }

    pub fn get_submissions(&self, show_score: bool) -> Vec<String> {
        let mut submissions: Vec<String> = vec![];
        for (id, clip) in self.submits.iter().enumerate() {
            if let Some(clip) = clip {
                let score = if let Some(s) = clip.rating {
                    format!("[{}/5]", s)
                } else {
                    "".to_string()
                };
                let clip_string = format!(
                    "{}{}- {} [{}]({})",
                    NUMBER_EMOJS[id],
                    if show_score {
                        format!(" {} ", score)
                    } else {
                        "".to_string()
                    },
                    clip.date.date_naive(),
                    clip.title,
                    clip.link
                );
                submissions.push(clip_string);
            }
        }
        submissions
    }

    // pub fn update_small_pity(&mut self, small_pity: i32) -> bool {
    //     if small_pity < 0 {
    //         return false;
    //     }
    //     self.wish.small_pity = small_pity;
    //     return true;
    // }

    // pub fn update_big_pity(&mut self, big_pity: i32) -> bool {
    //     if big_pity < 0 {
    //         return false;
    //     }
    //     self.wish.big_pity = big_pity;
    //     return true;
    // }

    // pub fn update_wishes(&mut self, wish_count: i32) -> bool {
    //     if wish_count < 0 {
    //         return false;
    //     }
    //     self.wish.wishes = wish_count;
    //     return true;
    // }
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
    pub pokedex: Vec<PokeData>,
    pub type_matrix: Vec<Vec<f32>>,
    pub type_name: Vec<String>,
    pub trainers: Vec<TrainerData>,
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
                    .description(format!("Welcome <@{}>! You are now registered with ProfessorBot, feel free to checkout Professors Commands in https://discord.com/channels/1236349546003566642/1236351799536123946", ctx.author().id))
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

        // EVENT DATA ////////////////////////////////////////////////////////////////////////////////////////
        let poke_string = read_lines("event/pokemon.txt");
        let mut pokedex = Vec::new();
        let missing_no = PokeData {
            name: "MissingNo.".to_string(),
            index: 0,
            desc: "????????????".to_string(),
            types: "Normal".to_string(),
            bsprite: "".to_string(),
            sprite: "https://archives.bulbagarden.net/media/upload/9/98/Missingno_RB.png"
                .to_string(),
            wallpaper: "https://cdn.discordapp.com/attachments/1260223476766343188/1262188839049625640/000.png?ex=6695b075&is=66945ef5&hm=2050fd6220766c821bdf1325567eca69f85e2236ca8861495efcb89cea201577&".to_string(),
            nickname: None,
            current_hp: None,
            health: None,
        };
        pokedex.push(missing_no);

        let mut poke_counter = 1;
        for poke_line in poke_string {
            let line_split: Vec<&str> = poke_line.split('*').collect();

            let poke_name: String = line_split
                .first()
                .unwrap_or_else(|| panic!("Failed to load Name for No. {}", poke_counter))
                .to_string();
            let poke_desc: String = line_split
                .get(1)
                .unwrap_or_else(|| panic!("Failed to load Description for No. {}", poke_counter))
                .to_string();
            let poke_types: String = line_split
                .get(2)
                .unwrap_or_else(|| panic!("Failed to load typing for No. {}", poke_counter))
                .to_string();
            let poke_sprite: String = line_split
                .get(3)
                .unwrap_or_else(|| panic!("Failed to load Sprite for No. {}", poke_counter))
                .to_string();
            let poke_wallpaper: String = line_split
                .get(4)
                .unwrap_or_else(|| panic!("Failed to load Wallpaper for No. {}", poke_counter))
                .to_string();
            let poke_back: String = line_split
                .get(5)
                .unwrap_or_else(|| panic!("Failed to load back sprite for No. {}", poke_counter))
                .to_string();

            let pokemon_info = PokeData {
                name: poke_name,
                index: poke_counter,
                desc: poke_desc,
                types: poke_types,
                sprite: poke_sprite,
                bsprite: poke_back,
                wallpaper: poke_wallpaper,
                nickname: None,
                current_hp: None,
                health: None,
            };

            pokedex.push(pokemon_info);
            poke_counter += 1;
        }

        let type_matrix = vec![
            vec![
                1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.5, 0.0, 1.0, 1.0,
                0.5, 1.0,
            ],
            vec![
                1.0, 0.5, 0.5, 1.0, 2.0, 2.0, 1.0, 1.0, 1.0, 1.0, 1.0, 2.0, 0.5, 1.0, 0.5, 1.0,
                2.0, 1.0,
            ],
            vec![
                1.0, 2.0, 0.5, 1.0, 0.5, 1.0, 1.0, 1.0, 2.0, 1.0, 1.0, 1.0, 2.0, 1.0, 0.5, 1.0,
                1.0, 1.0,
            ],
            vec![
                1.0, 1.0, 2.0, 0.5, 0.5, 1.0, 1.0, 1.0, 0.0, 2.0, 1.0, 1.0, 1.0, 1.0, 0.5, 1.0,
                1.0, 1.0,
            ],
            vec![
                1.0, 0.5, 2.0, 1.0, 0.5, 1.0, 1.0, 0.5, 2.0, 0.5, 1.0, 0.5, 2.0, 1.0, 0.5, 1.0,
                0.5, 1.0,
            ],
            vec![
                1.0, 0.5, 0.5, 1.0, 2.0, 0.5, 1.0, 1.0, 2.0, 2.0, 1.0, 1.0, 1.0, 1.0, 2.0, 1.0,
                0.5, 1.0,
            ],
            vec![
                2.0, 1.0, 1.0, 1.0, 1.0, 2.0, 1.0, 0.5, 1.0, 0.5, 0.5, 0.5, 2.0, 0.0, 1.0, 2.0,
                2.0, 0.5,
            ],
            vec![
                1.0, 1.0, 1.0, 1.0, 2.0, 1.0, 1.0, 0.5, 0.5, 1.0, 1.0, 1.0, 0.5, 0.5, 1.0, 1.0,
                0.0, 2.0,
            ],
            vec![
                1.0, 2.0, 1.0, 2.0, 0.5, 1.0, 1.0, 2.0, 1.0, 1.0, 1.0, 0.5, 2.0, 1.0, 1.0, 1.0,
                2.0, 1.0,
            ],
            vec![
                1.0, 1.0, 1.0, 0.5, 2.0, 1.0, 2.0, 1.0, 1.0, 1.0, 1.0, 2.0, 0.5, 1.0, 1.0, 1.0,
                0.5, 1.0,
            ],
            vec![
                1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 2.0, 2.0, 1.0, 1.0, 0.5, 1.0, 1.0, 1.0, 1.0, 0.0,
                0.5, 1.0,
            ],
            vec![
                1.0, 0.5, 1.0, 1.0, 2.0, 1.0, 0.5, 0.5, 1.0, 0.5, 2.0, 1.0, 1.0, 0.5, 1.0, 2.0,
                0.5, 0.5,
            ],
            vec![
                1.0, 2.0, 1.0, 1.0, 1.0, 2.0, 0.5, 1.0, 0.5, 2.0, 1.0, 2.0, 1.0, 1.0, 1.0, 1.0,
                0.5, 1.0,
            ],
            vec![
                0.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 2.0, 1.0, 1.0, 2.0, 1.0, 0.5,
                1.0, 1.0,
            ],
            vec![
                1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 2.0, 1.0,
                0.5, 0.0,
            ],
            vec![
                1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 0.5, 1.0, 1.0, 1.0, 2.0, 1.0, 1.0, 2.0, 1.0, 0.5,
                1.0, 0.5,
            ],
            vec![
                1.0, 0.5, 0.5, 0.5, 1.0, 2.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 2.0, 1.0, 1.0, 1.0,
                0.5, 2.0,
            ],
            vec![
                1.0, 0.5, 1.0, 1.0, 1.0, 1.0, 2.0, 0.5, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 2.0, 2.0,
                0.5, 1.0,
            ],
        ];

        let type_name: Vec<String> = vec![
            "Normal".to_string(),
            "Fire".to_string(),
            "Water".to_string(),
            "Electric".to_string(),
            "Grass".to_string(),
            "Ice".to_string(),
            "Fighting".to_string(),
            "Poison".to_string(),
            "Ground".to_string(),
            "Flying".to_string(),
            "Psychic".to_string(),
            "Bug".to_string(),
            "Rock".to_string(),
            "Ghost".to_string(),
            "Dragon".to_string(),
            "Dark".to_string(),
            "Steel".to_string(),
            "Fairy".to_string(),
        ];

        let trainer_string = read_lines("event/trainer.txt");
        let mut trainers = Vec::new();

        for (trainer_index, trainer_line) in trainer_string.into_iter().enumerate() {
            let line_split: Vec<&str> = trainer_line.split('=').collect();

            let trainer_name: String = line_split
                .first()
                .unwrap_or_else(|| panic!("Failed to load Name for No. {}", poke_counter))
                .to_string();
            let trainer_types: String = line_split
                .get(1)
                .unwrap_or_else(|| panic!("Failed to load Description for No. {}", poke_counter))
                .to_string();
            let trainer_wallpaper: String = line_split
                .get(2)
                .unwrap_or_else(|| panic!("Failed to load typing for No. {}", poke_counter))
                .to_string();

            let trainer_tier = if trainer_index < 15 {
                "Common".to_string()
            } else if (15..26).contains(&trainer_index) {
                "Mythic".to_string()
            } else {
                "Legendary".to_string()
            };

            let trainer_info = TrainerData {
                name: trainer_name,
                types: trainer_types,
                tier: trainer_tier,
                team: vec![],
                wallpaper: trainer_wallpaper,
            };

            trainers.push(trainer_info);
        }

        // EVENT DATA ////////////////////////////////////////////////////////////////////////////////////////

        Data {
            users,
            voice_users: Arc::new(DashMap::new()),
            meme,
            ponder,
            pong,
            d20f,
            mod_id,
            pokedex,
            type_matrix,
            type_name,
            trainers,
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
