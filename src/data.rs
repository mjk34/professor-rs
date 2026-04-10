//! Shared bot state, user data models, and global constants.
use crate::serenity;
use chrono::prelude::{DateTime, Utc};
use dashmap::DashMap;
use std::collections::VecDeque;
use poise::serenity_prelude::RoleId;
use serde::{Deserialize, Serialize};
use serenity::Color;
use std::sync::Arc;
use std::{env, fs};
use tokio::sync::RwLock;

// Professor AI memory
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub date: DateTime<Utc>,
    pub content: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProfessorMemory {
    pub core_behavior: String,
    pub entries: VecDeque<MemoryEntry>,
}

/// Minimum level required to unlock Gold Status and the elevated HYSA rate.
pub const GOLD_LEVEL_THRESHOLD: i32 = 10;
/// Annual HYSA interest rate (as a fraction) applied to uninvested cash for non-gold users.
pub const BASE_HYSA_RATE: f64 = 0.1;
/// Maximum number of trade history records retained per user before oldest entries are dropped.
pub const TRADE_HISTORY_LIMIT: usize = 500;
/// Maximum number of pending (queued) orders a user may have at once.
pub const MAX_PENDING_ORDERS: usize = 20;
/// Starting cred balance for every newly registered user (100,000 creds = $1,000 notional).
pub const NEW_USER_STARTING_CREDS: i32 = 100_000;

/// Discord keycap digit emoji 0–9, indexed by digit value. Used for ticket shop buttons.
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

pub const EMBED_DEFAULT: Color = Color::new(16_119_285); // white - transition color
pub const EMBED_CYAN: Color = Color::new(6_943_230); // cyan  - good finish color
pub const EMBED_GOLD: Color = Color::GOLD; // gold - cred related color
pub const EMBED_FAIL: Color = Color::RED; // red - absolute fails
pub const EMBED_LEVEL: Color = Color::ORANGE; // orange - level/xp related color
pub const EMBED_SUCCESS: Color = Color::new(65_280); // green - major success
pub const EMBED_ERROR: Color = Color::new(6_053_215); // grey - soft fails
pub const EMBED_MOD: Color = Color::new(16_749_300); // pink - moderator commands

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct ClipData {
    pub title: String,
    pub link: String,
    pub date: DateTime<Utc>,
    pub rating: Option<f64>,
}

impl ClipData {
    pub fn new(title: String, link: String) -> Self {
        Self {
            title,
            link,
            date: Utc::now(),
            rating: None,
        }
    }
}

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

    pub stock: StockProfile,

    #[serde(default)]
    pub professor_memory: Option<ProfessorMemory>,

    #[serde(default)]
    recent_rolls: VecDeque<i32>,
}

impl UserData {
    pub const fn update_level(&mut self) {
        self.level += 1;
    }

    pub const fn update_xp(&mut self, xp: i32) -> bool {
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

    pub const fn add_rolls(&mut self, roll: i32) -> bool {
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

    pub fn next_daily_timestamp(&self) -> i64 {
        (self.last_daily + chrono::Duration::hours(21)).timestamp()
    }

    pub fn add_bonus(&mut self) {
        self.bonus_count = (self.bonus_count + 1).min(3);
    }

    pub const fn reset_bonus(&mut self) {
        self.bonus_count = 0;
    }

    pub const fn check_claim(&self) -> bool {
        matches!(self.bonus_count, 3)
    }

    pub const fn add_creds(&mut self, creds: i32) -> bool {
        if creds < 0 {
            return false;
        }

        self.creds += creds;
        true
    }

    pub const fn sub_creds(&mut self, creds: i32) -> bool {
        if creds < 0 {
            return false;
        }
        self.creds -= creds;
        true
    }

    pub const fn add_tickets(&mut self, tickets: i32) -> bool {
        if tickets < 1 {
            return false;
        }

        self.tickets += tickets;
        true
    }

    pub const fn get_creds(&self) -> i32 {
        self.creds
    }

    pub const fn get_tickets(&self) -> i32 {
        self.tickets
    }

    pub fn get_luck(&self) -> String {
        if self.daily_count == 0 {
            return "N/A".to_string();
        }
        luck_label(self.get_luck_score()).to_string()
    }

    pub const fn get_luck_score(&self) -> i32 {
        self.rolls / (self.daily_count + 1)
    }

    pub fn push_roll(&mut self, d20: i32) {
        if self.recent_rolls.len() >= 7 {
            self.recent_rolls.pop_front();
        }
        self.recent_rolls.push_back(d20);
    }

    pub fn get_rolling_luck_score(&self) -> i32 {
        if self.recent_rolls.is_empty() {
            return 0;
        }
        self.recent_rolls.iter().sum::<i32>() / self.recent_rolls.len() as i32
    }

    pub fn get_rolling_luck(&self) -> String {
        if self.recent_rolls.is_empty() {
            return "N/A".to_string();
        }
        luck_label(self.get_rolling_luck_score()).to_string()
    }

    pub const fn get_bonus(&self) -> i32 {
        self.bonus_count
    }

    pub const fn get_level(&self) -> i32 {
        self.level
    }

    pub const fn get_xp(&self) -> i32 {
        self.xp
    }

    pub const fn get_next_level(&self) -> i32 {
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
        if submit_index >= self.submits.len() { return false; }
        self.submits.remove(submit_index).is_some()
    }

    pub fn get_submissions(&self, show_score: bool, show_icon: bool) -> Vec<String> {
        let mut submissions: Vec<String> = vec![];
        for (id, clip) in self.submits.iter().enumerate() {
            if let Some(clip) = clip {
                let score = if let Some(s) = clip.rating {
                    format!("[{s}/5]")
                } else {
                    "[-/5]".to_string()
                };
                let clip_string = format!(
                    "{} {} **[{}]({})** ({})",
                    if show_icon { NUMBER_EMOJS[id] } else { "" },
                    if show_score {
                        format!(" {score} ")
                    } else {
                        String::new()
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

fn luck_label(score: i32) -> &'static str {
    if score < 6 { "Horrible" }
    else if (6..9).contains(&score) { "Bad" }
    else if (9..12).contains(&score) { "Average" }
    else if (12..15).contains(&score) { "Good" }
    else { "Blessed" }
}

#[derive(Debug, Clone)]
pub struct VoiceUser {
    pub joined: DateTime<Utc>,
    pub last_reward: Option<DateTime<Utc>>,
    pub mute: Option<DateTime<Utc>>,
    pub deaf: Option<DateTime<Utc>>,
}

impl VoiceUser {
    pub fn new() -> Self {
        Self {
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

impl Default for VoiceUser {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Default, Debug, Serialize, Deserialize)]
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
#[derive(Debug)]
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
    pub gen_chat: String,
    pub bot_chat: String,
    pub sub_chat: String,
    pub bad_fortune: Vec<String>,
    pub good_fortune: Vec<String>,
    pub hysa_fed_rate: Arc<RwLock<f64>>,
    pub bot_user_id: serenity::UserId,
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

            let mut new_user = UserData::default();
            new_user.add_creds(NEW_USER_STARTING_CREDS);
            data.insert(user_id, Arc::new(RwLock::new(new_user)));
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
        save_users(&self.users).await;
    }

    /// Attempts to load the Data from a file, otherwise return a default
    pub fn load() -> Self {
        let data = fs::read_to_string("data.json").ok();
        let users_data: SaveData = if let Some(file) = data {
            match serde_json::from_str(&file) {
                Ok(d) => d,
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to deserialize data.json (schema mismatch?)");
                    SaveData::default()
                }
            }
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

        // Lame static fortunes until we get local LLM up to make funny ones
        let good_fortune = read_lines("reference/good_fortunes.txt");
        let bad_fortune = read_lines("reference/bad_fortunes.txt");

        let mod_id = RoleId::new(
            env::var("MOD_ID")
                .expect("Missing moderator ID")
                .parse()
                .unwrap(),
        );

        let gen_chat = env::var("GENERAL").expect("missing GENERAL id");
        let bot_chat = env::var("BOT_CMD").expect("missing BOT_CMD id");
        let sub_chat = env::var("SUBMIT").expect("missing SUBMIT id");
        Self {
            users,
            voice_users: Arc::new(DashMap::new()),
            meme,
            ponder,
            pong,
            d20f,
            mod_id,
            gen_chat,
            bot_chat,
            sub_chat,
            bad_fortune,
            good_fortune,
            hysa_fed_rate: Arc::new(RwLock::new(3.35)), // approximate fed funds rate at deployment; refreshed from FRED on startup
            bot_user_id: serenity::UserId::new(
                env::var("PROFESSOR")
                    .expect("missing PROFESSOR id")
                    .parse()
                    .expect("PROFESSOR id is not a valid u64"),
            ),
        }
    }
}

pub async fn save_users(
    users: &Arc<DashMap<serenity::UserId, Arc<RwLock<UserData>>>>,
) {
    let users_save = DashMap::new();

    for x in users.iter() {
        let (id, u) = x.pair();
        let u = u.read().await;
        users_save.insert(*id, u.clone());
    }

    let users_save = SaveData { users: users_save };

    let encoded = match serde_json::to_string(&users_save) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = %e, "save: serde_json encode failed — skipping write");
            return;
        }
    };
    if let Err(e) = tokio::fs::write("data.json", encoded).await {
        tracing::error!(error = %e, "save: data.json write failed");
    }
}

fn read_lines(filename: &str) -> Vec<String> {
    match fs::read_to_string(filename) {
        Ok(contents) => {
            let lines: Vec<String> = contents.lines().map(String::from).collect();
            tracing::info!(file = %filename, lines = lines.len(), "file loaded");
            lines
        }
        Err(e) => {
            tracing::warn!(file = %filename, error = %e, "could not read file — using empty list");
            Vec::new()
        }
    }
}

// ──────────────────────────────────────────────
// Stock / portfolio types
// ──────────────────────────────────────────────

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct StockProfile {
    pub portfolios: Vec<Portfolio>,
    pub trade_history: VecDeque<TradeRecord>,
    pub watchlist: Vec<String>,
    #[serde(default)]
    pub pending_orders: Vec<PendingOrder>,
    #[serde(default)]
    pub next_order_id: u32,
}

impl StockProfile {
    pub fn push_trade(&mut self, record: TradeRecord) {
        self.trade_history.push_back(record);
        if self.trade_history.len() > TRADE_HISTORY_LIMIT {
            self.trade_history.pop_front();
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Portfolio {
    pub name: String,
    pub cash: f64,
    pub last_interest_credited: DateTime<Utc>,
    pub positions: Vec<Position>,
    pub created_at: DateTime<Utc>,
}

impl Portfolio {
    pub fn new(name: String) -> Self {
        Self {
            name,
            cash: 0.0,
            last_interest_credited: Utc::now(),
            positions: Vec::new(),
            created_at: Utc::now(),
        }
    }

    /// Sum of collateral locked across all naked short option positions.
    pub fn locked_cash(&self) -> f64 {
        self.positions.iter().filter_map(|p| {
            if let AssetType::Option(c) = &p.asset_type {
                if c.side == OptionSide::Short { return Some(c.collateral); }
            }
            None
        }).sum()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub ticker: String,
    pub asset_type: AssetType,
    pub quantity: f64,
    pub avg_cost: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AssetType {
    Stock,
    #[expect(clippy::upper_case_acronyms, reason = "ETF is a well-known abbreviation, not a type name")]
    ETF,
    Crypto,
    Option(OptionContract),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OptionSide {
    Long,
    Short,
}

const fn default_long() -> OptionSide {
    OptionSide::Long
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionContract {
    pub strike: f64,
    pub expiry: DateTime<Utc>,
    pub option_type: OptionType,
    pub contracts: u32,
    #[serde(default = "default_long")]
    pub side: OptionSide,
    /// Total creds locked as margin collateral for naked short positions. 0 for covered/cash-secured.
    #[serde(default)]
    pub collateral: f64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OptionType {
    Call,
    Put,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeRecord {
    pub portfolio: String,
    pub ticker: String,
    pub asset_name: String,
    pub action: TradeAction,
    pub quantity: f64,
    pub price_per_unit: f64,
    pub total_creds: f64,
    pub realized_pnl: Option<f64>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradeAction {
    Buy,
    Sell,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderSide {
    Buy,
    Sell,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── UserData ──────────────────────────────────────────────────────────

    #[test]
    fn add_creds_normal_and_negative_guard() {
        let mut u = UserData::default();
        assert!(u.add_creds(500));
        assert_eq!(u.get_creds(), 500);
        assert!(!u.add_creds(-1));
        assert_eq!(u.get_creds(), 500); // unchanged
    }

    #[test]
    fn sub_creds_normal_and_negative_guard() {
        let mut u = UserData::default();
        u.add_creds(1000);
        assert!(u.sub_creds(400));
        assert_eq!(u.get_creds(), 600);
        assert!(!u.sub_creds(-1));
        assert_eq!(u.get_creds(), 600); // unchanged
    }

    #[test]
    fn update_xp_no_levelup() {
        let mut u = UserData::default();
        let leveled = u.update_xp(100);
        assert!(!leveled);
        assert_eq!(u.get_xp(), 100);
        assert_eq!(u.get_level(), 0);
    }

    #[test]
    fn update_xp_exact_threshold_levels_up() {
        let mut u = UserData::default();
        // Level 0 threshold: 500 + 0 * 80 = 500
        let leveled = u.update_xp(500);
        assert!(leveled);
        assert_eq!(u.get_level(), 1);
        assert_eq!(u.get_xp(), 0); // no leftover
    }

    #[test]
    fn update_xp_carries_over_excess() {
        let mut u = UserData::default();
        // Level 0 threshold = 500; give 550 — should level up with 50 leftover
        let leveled = u.update_xp(550);
        assert!(leveled);
        assert_eq!(u.get_level(), 1);
        assert_eq!(u.get_xp(), 50);
    }

    #[test]
    fn update_xp_threshold_increases_with_level() {
        let mut u = UserData::default();
        u.update_xp(500); // level 0 → 1; threshold was 500
        // Level 1 threshold: 500 + 1 * 80 = 580
        assert_eq!(u.get_next_level(), 580);
        let leveled = u.update_xp(579);
        assert!(!leveled);
        assert_eq!(u.get_level(), 1);
    }

    #[test]
    fn update_xp_negative_is_noop() {
        let mut u = UserData::default();
        assert!(!u.update_xp(-1));
        assert_eq!(u.get_xp(), 0);
        assert_eq!(u.get_level(), 0);
    }

    #[test]
    fn luck_tiers_at_boundaries() {
        let mut u = UserData::default();
        u.daily_count = 1;

        u.rolls = 5;  assert_eq!(u.get_luck(), "Horrible"); // score = 5/2 = 2
        u.rolls = 12; assert_eq!(u.get_luck(), "Bad");      // score = 12/2 = 6
        u.rolls = 18; assert_eq!(u.get_luck(), "Average");  // score = 18/2 = 9
        u.rolls = 24; assert_eq!(u.get_luck(), "Good");     // score = 24/2 = 12
        u.rolls = 30; assert_eq!(u.get_luck(), "Blessed");  // score = 30/2 = 15
    }

    #[test]
    fn luck_no_dailies_returns_na() {
        let u = UserData::default();
        assert_eq!(u.get_luck(), "N/A");
    }

    #[test]
    fn push_roll_capped_at_seven() {
        let mut u = UserData::default();
        for i in 1..=10 {
            u.push_roll(i);
        }
        // Only last 7 should remain: 4..=10
        assert_eq!(u.recent_rolls.len(), 7);
        assert_eq!(*u.recent_rolls.front().unwrap(), 4);
        assert_eq!(*u.recent_rolls.back().unwrap(), 10);
    }

    #[test]
    fn rolling_luck_score_averages_correctly() {
        let mut u = UserData::default();
        u.push_roll(10);
        u.push_roll(20);
        // avg = 30 / 2 = 15 → Blessed
        assert_eq!(u.get_rolling_luck_score(), 15);
        assert_eq!(u.get_rolling_luck(), "Blessed");
    }

    #[test]
    fn rolling_luck_empty_returns_na() {
        let u = UserData::default();
        assert_eq!(u.get_rolling_luck(), "N/A");
        assert_eq!(u.get_rolling_luck_score(), 0);
    }

    #[test]
    fn bonus_counter_caps_at_three_and_check_claim() {
        let mut u = UserData::default();
        assert!(!u.check_claim());
        u.add_bonus(); u.add_bonus(); u.add_bonus();
        assert_eq!(u.get_bonus(), 3);
        assert!(u.check_claim());
        u.add_bonus(); // should not exceed 3
        assert_eq!(u.get_bonus(), 3);
        u.reset_bonus();
        assert_eq!(u.get_bonus(), 0);
        assert!(!u.check_claim());
    }

    // ── Portfolio ─────────────────────────────────────────────────────────

    fn make_short_option(collateral: f64) -> Position {
        Position {
            ticker: "TEST".to_string(),
            asset_type: AssetType::Option(OptionContract {
                strike: 100.0,
                expiry: Utc::now(),
                option_type: OptionType::Call,
                contracts: 1,
                side: OptionSide::Short,
                collateral,
            }),
            quantity: 1.0,
            avg_cost: 0.0,
        }
    }

    fn make_long_option(collateral: f64) -> Position {
        Position {
            ticker: "TEST".to_string(),
            asset_type: AssetType::Option(OptionContract {
                strike: 100.0,
                expiry: Utc::now(),
                option_type: OptionType::Put,
                contracts: 1,
                side: OptionSide::Long,
                collateral,
            }),
            quantity: 1.0,
            avg_cost: 0.0,
        }
    }

    fn make_stock_position() -> Position {
        Position {
            ticker: "AAPL".to_string(),
            asset_type: AssetType::Stock,
            quantity: 10.0,
            avg_cost: 500.0,
        }
    }

    #[test]
    fn locked_cash_sums_only_short_collateral() {
        let mut port = Portfolio::new("test".to_string());
        port.positions.push(make_short_option(1000.0));
        port.positions.push(make_short_option(500.0));
        port.positions.push(make_long_option(999.0));  // should not count
        port.positions.push(make_stock_position());     // should not count
        assert_eq!(port.locked_cash(), 1500.0);
    }

    #[test]
    fn locked_cash_empty_portfolio() {
        let port = Portfolio::new("empty".to_string());
        assert_eq!(port.locked_cash(), 0.0);
    }

    // ── StockProfile ──────────────────────────────────────────────────────

    #[test]
    fn push_trade_enforces_history_limit() {
        let mut sp = StockProfile::default();
        for i in 0..=TRADE_HISTORY_LIMIT {
            sp.push_trade(TradeRecord {
                portfolio: "p".to_string(),
                ticker: format!("T{i}"),
                asset_name: "name".to_string(),
                action: TradeAction::Buy,
                quantity: 1.0,
                price_per_unit: 100.0,
                total_creds: 100.0,
                realized_pnl: None,
                timestamp: Utc::now(),
            });
        }
        assert_eq!(sp.trade_history.len(), TRADE_HISTORY_LIMIT);
        // oldest entry (T0) should have been dropped
        assert_eq!(sp.trade_history.front().unwrap().ticker, "T1");
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingOrder {
    pub id: u32,
    pub side: OrderSide,
    pub ticker: String,
    pub asset_name: String,
    pub asset_type: AssetType,
    pub portfolio_name: String,
    pub quantity: f64,
    /// None = market order (queued for next open); Some = limit price in USD
    pub limit_price: Option<f64>,
    pub expiry: DateTime<Utc>,
}
