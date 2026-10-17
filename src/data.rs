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

/// Starting cred balance for every newly registered user (100,000 creds = $1,000 notional).
pub const NEW_USER_STARTING_CREDS: i32 = 100_000;

// ── Economy roll constants ────────────────────────────────────────────────────
/// Daily roll: d20 range (1..=20).
pub const D20_RANGE: std::ops::Range<i32> = 1..21;
/// DC check range for daily/bonus rolls.
pub const DC_CHECK_RANGE: std::ops::Range<i32> = 6..15;
/// Daily fortune: base creds added before DC scaling.
pub const DAILY_FORTUNE_BASE: i32 = 2_000;
/// Daily fortune: creds per DC step.
pub const DAILY_FORTUNE_STEP: i32 = 600;
/// Bonus fortune: base creds added before DC scaling.
pub const BONUS_FORTUNE_BASE: i32 = 3_000;
/// Bonus fortune: creds per DC step.
pub const BONUS_FORTUNE_STEP: i32 = 900;
/// Crit success (nat 20) creds range for daily roll.
pub const DAILY_CRIT_SUCCESS: std::ops::Range<i32> = 50_000..200_000;
/// Crit fail (nat 1) creds range for daily roll (subtracted).
pub const DAILY_CRIT_FAIL: std::ops::Range<i32> = 15_000..40_000;
/// Crit success creds range for bonus roll.
pub const BONUS_CRIT_SUCCESS: std::ops::Range<i32> = 35_000..120_000;
/// XP awarded per daily roll.
pub const DAILY_XP: i32 = 500;
/// XP awarded per bonus claim.
pub const BONUS_XP: i32 = 150;
/// Ticket base cost.
pub const TICKET_BASE_COST: i32 = 2_000;
/// Ticket cost increment per ticket already owned.
pub const TICKET_COST_STEP: i32 = 300;
/// Maximum tickets purchasable in one `/buy_tickets` transaction.
pub const MAX_TICKET_PURCHASE: i32 = 100;

// ── Progression constants ─────────────────────────────────────────────────────
/// XP needed to go from level 0 to level 1.
const XP_BASE: i32 = 500;
/// Extra XP required per level already gained (cap = `XP_BASE + level * XP_PER_LEVEL`).
const XP_PER_LEVEL: i32 = 80;
/// Hours between daily rolls — 21 rather than 24 so the daily doesn't drift later each day.
const DAILY_COOLDOWN_HOURS: i64 = 21;
/// Dailies needed to unlock `/claim_bonus`.
pub const BONUS_CLAIM_THRESHOLD: i32 = 3;
/// Minutes in voice per voice reward.
const VOICE_REWARD_MINUTES: i64 = 30;
/// Number of recent d20 rolls averaged for the rolling-luck score.
const ROLLING_LUCK_WINDOW: usize = 7;
/// Maximum clip submissions per user (also bounded by the 10 `NUMBER_EMOJS` keycaps).
const MAX_CLIP_SUBMISSIONS: usize = 5;

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

/// White — in-progress/transition embeds (e.g. "rolling…").
pub const EMBED_DEFAULT: Color = Color::new(16_119_285);
/// Cyan — successful completion.
pub const EMBED_CYAN: Color = Color::new(6_943_230);
/// Gold — cred-related embeds and crit successes.
pub const EMBED_GOLD: Color = Color::GOLD;
/// Red — hard failures (crit fails, limits hit).
pub const EMBED_FAIL: Color = Color::RED;
/// Orange — level/XP embeds.
pub const EMBED_LEVEL: Color = Color::ORANGE;
/// Green — passed rolls.
pub const EMBED_SUCCESS: Color = Color::new(65_280);
/// Grey — soft failures (cooldowns, not ready, timed out).
pub const EMBED_ERROR: Color = Color::new(6_053_215);
/// Pink — moderator commands.
pub const EMBED_MOD: Color = Color::new(16_749_300);

/// Clip ratings are 1..=`MAX_CLIP_RATING` stars.
pub const MAX_CLIP_RATING: u8 = 5;

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ClipData {
    pub title: String,
    pub link: String,
    /// Also the clip's identity (see `UserData::clip_mut`). Defaults to "now" rather than the
    /// epoch so clips missing a date can't share one.
    #[serde(default = "Utc::now")]
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

    /// `[title](link)` safe for any embed: the title is escaped and the link has the
    /// characters that could close the target (or start a new link) percent-encoded, so
    /// links stored before submit-time validation was tightened can't inject markdown either.
    pub fn markdown_link(&self) -> String {
        let link = self.link.replace('(', "%28").replace(')', "%29").replace(' ', "%20");
        format!("[{}]({link})", clip_title_display(&self.title))
    }
}

/// `#[serde(default)]` so a field added later loads as its default instead of failing the
/// whole `data.json` parse.
#[derive(Default, Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
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

        self.xp = self.xp.saturating_add(xp);
        let xp_cap = self.get_next_level();

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

        self.rolls = self.rolls.saturating_add(roll);
        true
    }

    pub fn check_daily(&self) -> bool {
        self.check_daily_at(Utc::now())
    }

    fn check_daily_at(&self, now: DateTime<Utc>) -> bool {
        (now - self.last_daily).num_hours() >= DAILY_COOLDOWN_HOURS
    }

    pub fn next_daily_timestamp(&self) -> i64 {
        (self.last_daily + chrono::Duration::hours(DAILY_COOLDOWN_HOURS)).timestamp()
    }

    pub fn add_bonus(&mut self) {
        self.bonus_count = (self.bonus_count + 1).min(BONUS_CLAIM_THRESHOLD);
    }

    pub const fn reset_bonus(&mut self) {
        self.bonus_count = 0;
    }

    pub const fn check_claim(&self) -> bool {
        self.bonus_count == BONUS_CLAIM_THRESHOLD
    }

    pub const fn add_creds(&mut self, creds: i32) -> bool {
        if creds < 0 {
            return false;
        }

        self.creds = self.creds.saturating_add(creds);
        true
    }

    pub const fn sub_creds(&mut self, creds: i32) -> bool {
        if creds < 0 {
            return false;
        }
        self.creds = self.creds.saturating_sub(creds);
        true
    }

    pub const fn add_tickets(&mut self, tickets: i32) -> bool {
        if tickets < 1 {
            return false;
        }

        self.tickets = self.tickets.saturating_add(tickets);
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

    /// Mean d20 roll across all dailies; 0 before the first daily.
    pub const fn get_luck_score(&self) -> i32 {
        if self.daily_count == 0 {
            return 0;
        }
        self.rolls / self.daily_count
    }

    pub fn push_roll(&mut self, d20: i32) {
        if self.recent_rolls.len() >= ROLLING_LUCK_WINDOW {
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
        XP_BASE + self.get_level() * XP_PER_LEVEL
    }

    pub fn add_submit(&mut self, new_submit: ClipData) -> bool {
        for i in 0..MAX_CLIP_SUBMISSIONS {
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

    /// The clip submitted at `date`. Submission time identifies a clip: slots are reused, so
    /// an index captured by a long-lived `/next_clip` vote or `/my_clips` button could point
    /// at a newer clip by the time it's used.
    fn clip_mut(&mut self, date: DateTime<Utc>) -> Option<&mut Option<ClipData>> {
        self.submits.iter_mut().find(|s| s.as_ref().is_some_and(|c| c.date == date))
    }

    /// Empties the clip's slot (later slots keep their positions, and the slot is reused).
    pub fn remove_submit(&mut self, date: DateTime<Utc>) -> bool {
        self.clip_mut(date).and_then(Option::take).is_some()
    }

    pub fn rate_submit(&mut self, date: DateTime<Utc>, rating: f64) -> bool {
        match self.clip_mut(date) {
            Some(Some(clip)) => {
                clip.rating = Some(rating);
                true
            }
            _ => false,
        }
    }

    pub fn has_submissions(&self) -> bool {
        self.submits.iter().any(Option::is_some)
    }

    pub fn get_submissions(&self, style: SubmissionStyle) -> Vec<String> {
        let show_score = style == SubmissionStyle::Scored;
        let show_icon = style == SubmissionStyle::Numbered;
        let mut submissions: Vec<String> = vec![];
        for (id, clip) in self.submits.iter().enumerate() {
            if let Some(clip) = clip {
                let score = if let Some(s) = clip.rating {
                    format!("[{s}/{MAX_CLIP_RATING}]")
                } else {
                    format!("[-/{MAX_CLIP_RATING}]")
                };
                let clip_string = format!(
                    "{} {} **{}** ({})",
                    if show_icon { NUMBER_EMOJS.get(id).copied().unwrap_or("") } else { "" },
                    if show_score {
                        format!(" {score} ")
                    } else {
                        String::new()
                    },
                    clip.markdown_link(),
                    clip.date.format("%m/%d")
                );
                submissions.push(clip_string);
            }
        }
        submissions
    }
}

/// How `UserData::get_submissions` renders each clip line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmissionStyle {
    /// Prefixed with the clip's rating — the moderator overview.
    Scored,
    /// Prefixed with the slot's keycap emoji, matching the delete buttons.
    Numbered,
}

/// Max clip title length accepted by `/submit_clip`; also the display cap for older titles.
pub const MAX_CLIP_TITLE_LEN: usize = 100;

/// Clip title made safe for an embed: truncated and markdown-escaped so it can't break
/// out of the `[title](link)` masked link.
pub fn clip_title_display(title: &str) -> String {
    let end = title.char_indices().nth(MAX_CLIP_TITLE_LEN).map_or(title.len(), |(i, _)| i);
    crate::helper::escape_markdown(&title[..end])
}

/// Luck tier for a mean d20 roll. Bands are three faces wide, centred on the d20 mean
/// (10.5): below 6 "Horrible", 6–8 "Bad", 9–11 "Average", 12–14 "Good", 15+ "Blessed".
fn luck_label(score: i32) -> &'static str {
    if score < 6 { "Horrible" }
    else if (6..9).contains(&score) { "Bad" }
    else if (9..12).contains(&score) { "Average" }
    else if (12..15).contains(&score) { "Good" }
    else { "Blessed" }
}

#[derive(Debug, Clone)]
pub struct VoiceUser {
    /// Guild of the voice channel, so a guild's snapshot can prune only its own entries.
    pub guild_id: Option<serenity::GuildId>,
    pub joined: DateTime<Utc>,
    pub last_reward: Option<DateTime<Utc>>,
    pub mute: Option<DateTime<Utc>>,
    pub deaf: Option<DateTime<Utc>>,
}

impl VoiceUser {
    pub fn new() -> Self {
        Self {
            guild_id: None,
            joined: Utc::now(),
            last_reward: None,
            mute: None,
            deaf: None,
        }
    }
    /// Whether a voice reward is due at `now`; if so, starts the next interval. Deafened
    /// members aren't participating: they earn nothing, and deafened time doesn't count.
    pub fn take_reward_due(&mut self, now: DateTime<Utc>) -> bool {
        if self.deaf.is_some() {
            self.last_reward = Some(now);
            return false;
        }
        let since = self.last_reward.unwrap_or(self.joined);
        if (now - since).num_minutes() < VOICE_REWARD_MINUTES {
            return false;
        }
        self.last_reward = Some(now);
        true
    }

    pub fn update_mute(&mut self, b: bool) {
        track_since(&mut self.mute, b);
    }
    pub fn update_deaf(&mut self, b: bool) {
        track_since(&mut self.deaf, b);
    }
}

/// Records when a state began. Keeps the original timestamp while it stays on — voice-state
/// events also fire for unrelated changes (streaming, video) and must not restart the timer.
fn track_since(since: &mut Option<DateTime<Utc>>, on: bool) {
    if on {
        since.get_or_insert_with(Utc::now);
    } else {
        *since = None;
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

/// Shared map of every registered user's data.
pub type UsersMap = Arc<DashMap<serenity::UserId, Arc<RwLock<UserData>>>>;

/// User data, which is stored and accessible in all command invocations
#[derive(Debug)]
pub struct Data {
    /// Persistent data of users
    pub users: UsersMap,
    /// Duration of users in voice channel, updates by events
    pub voice_users: Arc<DashMap<serenity::UserId, VoiceUser>>,
    pub meme: Vec<String>,
    pub ponder: Vec<String>,
    pub pong: Vec<String>,
    pub d20f: Vec<String>,
    pub mod_id: RoleId,
    pub gen_chat: serenity::ChannelId,
    pub bot_chat: serenity::ChannelId,
    pub sub_chat: serenity::ChannelId,
    pub bad_fortune: Vec<String>,
    pub good_fortune: Vec<String>,
}

impl Data {
    /// Returns the user's data handle, registering them with starting creds if new.
    /// The flag is `true` when the account was just created. Uses the entry API so two
    /// concurrent first commands can't both insert (and one overwrite the other).
    pub fn get_or_create_user(&self, user_id: serenity::UserId) -> (Arc<RwLock<UserData>>, bool) {
        let mut created = false;
        let entry = self.users.entry(user_id).or_insert_with(|| {
            created = true;
            let mut new_user = UserData::default();
            new_user.add_creds(NEW_USER_STARTING_CREDS);
            Arc::new(RwLock::new(new_user))
        });
        (Arc::clone(entry.value()), created)
    }

    pub async fn check_or_create_user(ctx: crate::Context<'_>) -> Result<(), crate::Error> {
        let (_, created) = ctx.data().get_or_create_user(ctx.author().id);
        if !created {
            return Ok(());
        }

        send_welcome(ctx, ctx.author().id).await
    }

    /// Attempts to save the data to a file
    pub async fn save(&self) {
        save_users(&self.users).await;
    }

    /// Loads `data.json` and the environment config, starting empty only when the data file
    /// doesn't exist yet.
    ///
    /// # Errors
    /// An unreadable or unparseable data file is an error rather than an empty database — the
    /// next save would otherwise overwrite every user's data. A copy of the bad file is kept
    /// as `data.json.corrupt-<unix time>` for recovery. Missing or invalid `MOD_ID`,
    /// `GENERAL`, `BOT_CMD` or `SUBMIT` ids are also errors.
    pub fn load() -> Result<Self, crate::Error> {
        let users_data: SaveData = match fs::read_to_string(DATA_FILE) {
            Ok(file) => match serde_json::from_str(&file) {
                Ok(d) => d,
                Err(e) => {
                    let backup = format!("{DATA_FILE}.corrupt-{}", Utc::now().timestamp());
                    let copied = fs::copy(DATA_FILE, &backup).map(|_| backup);
                    return Err(format!("{DATA_FILE} failed to parse ({e}); backup: {copied:?}").into());
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                tracing::warn!(file = DATA_FILE, "data file not found — starting with an empty database");
                SaveData::default()
            }
            Err(e) => return Err(format!("could not read {DATA_FILE}: {e}").into()),
        };

        let users = Arc::new(DashMap::default());
        for x in &users_data.users {
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

        let mod_id = RoleId::new(env_id("MOD_ID")?);
        let gen_chat = serenity::ChannelId::new(env_id("GENERAL")?);
        let bot_chat = serenity::ChannelId::new(env_id("BOT_CMD")?);
        let sub_chat = serenity::ChannelId::new(env_id("SUBMIT")?);
        Ok(Self {
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
        })
    }
}

/// Channel where new users are pointed to read the bot's command list.
const COMMANDS_CHANNEL_URL: &str = "https://discord.com/channels/859993171156140061/860013281165967380";

/// Announces a newly created account. Shared by `pre_command` registration and moderators
/// crediting someone who has never used the bot.
pub async fn send_welcome(ctx: crate::Context<'_>, user_id: serenity::UserId) -> Result<(), crate::Error> {
    ctx.send(
        poise::CreateReply::default()
            .content(format!("<@{user_id}>"))
            .embed(
                serenity::CreateEmbed::new()
                    .title("Account Created!")
                    .description(format!("Welcome <@{user_id}>! You are now registered with ProfessorBot, feel free to checkout Professors Commands in {COMMANDS_CHANNEL_URL}"))
                    .image("https://gifdb.com/images/high/anime-girl-okay-sign-b5zlye5h8mnjhdg2.gif")
                    .color(EMBED_DEFAULT),
            ),
    )
    .await?;
    Ok(())
}

/// Serializes saves: `post_command` and the maintenance task can both call `save_users`.
static SAVE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
/// Persistent user database, relative to the working directory.
const DATA_FILE: &str = "data.json";

pub async fn save_users(
    users: &UsersMap,
) {
    let _save_guard = SAVE_LOCK.lock().await;

    // Snapshot the handles first — DashMap shard locks must not be held across `.await`.
    let handles: Vec<_> = users.iter().map(|e| (*e.key(), Arc::clone(e.value()))).collect();
    let users_save = DashMap::new();
    for (id, u) in handles {
        users_save.insert(id, u.read().await.clone());
    }

    let users_save = SaveData { users: users_save };

    let encoded = match serde_json::to_string(&users_save) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(error = %e, "save: serde_json encode failed — skipping write");
            return;
        }
    };
    crate::helper::write_atomic(DATA_FILE, encoded).await;
}

/// Required Discord id (role or channel) from the environment.
fn env_id(key: &str) -> Result<u64, crate::Error> {
    let raw = env::var(key).map_err(|_| format!("missing {key} in environment"))?;
    crate::helper::parse_id(&raw).ok_or_else(|| format!("{key} is not a valid Discord id").into())
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
        let mut u = UserData { daily_count: 2, ..Default::default() };

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
    fn luck_score_is_mean_roll() {
        // One daily with a nat 20 averages to 20, not 20 / 2.
        let u = UserData { daily_count: 1, rolls: 20, ..Default::default() };
        assert_eq!(u.get_luck_score(), 20);
        assert_eq!(UserData::default().get_luck_score(), 0);
    }

    #[test]
    fn creds_saturate_instead_of_overflowing() {
        let mut u = UserData { creds: i32::MAX - 1, ..Default::default() };
        u.add_creds(10);
        assert_eq!(u.get_creds(), i32::MAX);
        u.creds = i32::MIN + 1;
        u.sub_creds(10);
        assert_eq!(u.get_creds(), i32::MIN);
    }

    #[test]
    fn remove_submit_keeps_other_slot_indices() {
        let mut u = UserData::default();
        for t in ["a", "b", "c"] {
            assert!(u.add_submit(ClipData::new(t.into(), String::new())));
        }
        let a_date = u.submits[0].as_ref().unwrap().date;
        assert!(u.remove_submit(a_date));
        assert!(!u.remove_submit(a_date)); // already removed
        assert_eq!(u.submits[1].as_ref().unwrap().title, "b");
        assert_eq!(u.submits[2].as_ref().unwrap().title, "c");
        // The freed slot is reused by the next submission.
        assert!(u.add_submit(ClipData::new("d".into(), String::new())));
        assert_eq!(u.submits[0].as_ref().unwrap().title, "d");
        assert!(u.has_submissions());
        // A stale reference to the removed clip must not touch the clip now in its slot.
        assert!(!u.rate_submit(a_date, 5.0));
        assert!(u.submits[0].as_ref().unwrap().rating.is_none());
    }

    #[test]
    fn markdown_link_cannot_break_out() {
        let clip = ClipData::new("t".into(), "https://youtu.be/x)[a](https://evil)".into());
        assert_eq!(clip.markdown_link(), "[t](https://youtu.be/x%29[a]%28https://evil%29)");
    }

    #[test]
    fn clip_title_display_escapes_and_truncates() {
        assert_eq!(clip_title_display("[x](https://evil)"), r"\[x\]\(https://evil\)");
        assert_eq!(clip_title_display(&"a".repeat(500)).len(), MAX_CLIP_TITLE_LEN);
    }

    #[test]
    fn voice_reward_every_interval_but_not_while_deafened() {
        let mut v = VoiceUser::new();
        let start = v.joined;
        let mins = chrono::Duration::minutes;
        assert!(!v.take_reward_due(start + mins(VOICE_REWARD_MINUTES - 1)));
        assert!(v.take_reward_due(start + mins(VOICE_REWARD_MINUTES)));
        assert!(!v.take_reward_due(start + mins(VOICE_REWARD_MINUTES + 1))); // interval restarted
        // Deafened for an hour: no reward, and that hour doesn't count toward the next one.
        v.update_deaf(true);
        assert!(!v.take_reward_due(start + mins(VOICE_REWARD_MINUTES * 3)));
        v.update_deaf(false);
        assert!(!v.take_reward_due(start + mins(VOICE_REWARD_MINUTES * 3 + 1)));
        assert!(v.take_reward_due(start + mins(VOICE_REWARD_MINUTES * 4)));
    }

    #[test]
    fn mute_timestamp_survives_repeat_events() {
        let mut v = VoiceUser::new();
        v.update_mute(true);
        let first = v.mute;
        v.update_mute(true);
        assert_eq!(v.mute, first);
        v.update_mute(false);
        assert!(v.mute.is_none());
    }

    #[test]
    fn user_data_missing_fields_default() {
        let u: UserData = serde_json::from_str(r#"{"creds": 42}"#).unwrap();
        assert_eq!(u.get_creds(), 42);
        assert!(u.recent_rolls.is_empty());
    }

    #[test]
    fn add_submit_rejects_past_cap() {
        let mut u = UserData::default();
        for i in 0..MAX_CLIP_SUBMISSIONS {
            assert!(u.add_submit(ClipData::new(format!("c{i}"), String::new())));
        }
        assert!(!u.add_submit(ClipData::new("extra".into(), String::new())));
        assert_eq!(u.submits.len(), MAX_CLIP_SUBMISSIONS);
    }

    #[test]
    fn get_submissions_styles() {
        let mut u = UserData::default();
        u.add_submit(ClipData::new("a".into(), "https://youtu.be/a".into()));
        u.add_submit(ClipData { rating: Some(4.5), ..ClipData::new("b".into(), "https://youtu.be/b".into()) });
        let scored = u.get_submissions(SubmissionStyle::Scored);
        assert!(scored[0].contains("[-/5]") && scored[1].contains("[4.5/5]"));
        assert!(!scored[0].contains(NUMBER_EMOJS[0]));
        let numbered = u.get_submissions(SubmissionStyle::Numbered);
        assert!(numbered[1].starts_with(NUMBER_EMOJS[1]) && !numbered[1].contains("/5]"));
    }

    #[test]
    fn daily_cooldown_boundary() {
        let last = Utc::now();
        let u = UserData { last_daily: last, ..Default::default() };
        let h = chrono::Duration::hours;
        assert!(!u.check_daily_at(last + h(DAILY_COOLDOWN_HOURS) - chrono::Duration::minutes(1)));
        assert!(u.check_daily_at(last + h(DAILY_COOLDOWN_HOURS)));
        assert_eq!(u.next_daily_timestamp(), (last + h(DAILY_COOLDOWN_HOURS)).timestamp());
    }

    #[test]
    fn save_data_round_trips() {
        let mut u = UserData { level: 3, xp: 10, creds: 1234, daily_count: 2, rolls: 30, tickets: 4, ..Default::default() };
        u.push_roll(17);
        u.add_submit(ClipData { rating: Some(3.0), ..ClipData::new("t".into(), "https://youtu.be/x".into()) });
        let save = SaveData::default();
        save.users.insert(serenity::UserId::new(42), u);
        let json = serde_json::to_string(&save).unwrap();
        let back: SaveData = serde_json::from_str(&json).unwrap();
        let back = back.users.get(&serenity::UserId::new(42)).unwrap().clone();
        assert_eq!(serde_json::to_string(&back).unwrap(), serde_json::to_string(&*save.users.get(&serenity::UserId::new(42)).unwrap()).unwrap());
    }

    #[test]
    fn clips_missing_dates_get_distinct_identities() {
        let a: ClipData = serde_json::from_str(r#"{"title":"a","link":"x"}"#).unwrap();
        let b: ClipData = serde_json::from_str(r#"{"title":"b","link":"y"}"#).unwrap();
        assert_ne!(a.date, b.date);
        assert_ne!(a.date, DateTime::<Utc>::default());
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
}

