//! /leaderboard command — creds and fortune rankings with pagination.

use crate::{data, serenity, Context, Error};
use crate::helper::{default_footer, fetch_users, user_name};
use poise::serenity_prelude::{EditMessage, futures, UserId};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone, Copy, PartialEq)]
enum Sort { Creds, Fortune }

impl Sort {
    const fn title(self) -> &'static str {
        match self {
            Self::Creds   => "Leaderboard — Creds",
            Self::Fortune => "Leaderboard — Rolling Fortune",
        }
    }
}

/// Rows shown per leaderboard page.
const PAGE_SIZE: usize = 10;

/// (`user_id`, `sort_key`, `display_label`)
type Entry = (UserId, i64, String);

/// (username, avatar URL) per user. Filled lazily for the rows actually shown, so a
/// leaderboard costs at most one page of user lookups per view instead of one per user.
type UserMeta = HashMap<UserId, (String, String)>;

const fn page_count(entries: &[Entry]) -> usize {
    let pages = entries.len().div_ceil(PAGE_SIZE);
    if pages == 0 { 1 } else { pages }
}

fn build_page(entries: &[Entry], meta: &UserMeta, page: usize) -> String {
    let start = page * PAGE_SIZE;
    let mut text = String::from("﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n```\n");
    for (rank, (id, _, label)) in entries.iter().enumerate().skip(start).take(PAGE_SIZE) {
        let name = meta.get(id).map_or("Unknown", |(n, _)| n.as_str());
        let name_col: String = name.chars().take(16).collect();
        text.push_str(&format!("{:<4} {:^16} {:>18}\n", format!("#{}", rank + 1), name_col, label));
    }
    text.push_str("```");
    text
}

fn make_components(active: Sort) -> Vec<serenity::CreateActionRow> {
    use poise::serenity_prelude::ButtonStyle::{Primary, Secondary};
    vec![serenity::CreateActionRow::Buttons(vec![
        serenity::CreateButton::new("lb_creds")
            .label("Creds")
            .style(if active == Sort::Creds { Primary } else { Secondary }),
        serenity::CreateButton::new("lb_fortune")
            .label("Fortune")
            .style(if active == Sort::Fortune { Primary } else { Secondary }),
        serenity::CreateButton::new("lb_back").label("<").style(Secondary),
        serenity::CreateButton::new("lb_next").label(">").style(Secondary),
    ])]
}

fn make_embed(entries: &[Entry], meta: &UserMeta, sort: Sort, page: usize) -> serenity::CreateEmbed {
    let total_pages = page_count(entries);
    let thumbnail = entries.first()
        .and_then(|(id, ..)| meta.get(id))
        .map(|(_, avatar)| avatar.clone())
        .unwrap_or_default();
    serenity::CreateEmbed::new()
        .title(sort.title())
        .color(data::EMBED_CYAN)
        .thumbnail(thumbnail)
        .description("Here lists the most accomplished in UwUversity!")
        .field("Rankings", build_page(entries, meta, page), false)
        .field("Page", format!("{}/{}", page + 1, total_pages), false)
        .footer(default_footer())
}

fn empty_embed(sort: Sort) -> serenity::CreateEmbed {
    let desc = match sort {
        Sort::Fortune => "No fortune data yet — users need to /uwu first.",
        Sort::Creds   => "No users found.",
    };
    serenity::CreateEmbed::new()
        .title(sort.title())
        .color(data::EMBED_ERROR)
        .description(desc)
        .footer(default_footer())
}

struct Boards {
    creds:   Vec<Entry>,
    fortune: Vec<Entry>,
}

impl Boards {
    fn get(&self, sort: Sort) -> &[Entry] {
        match sort {
            Sort::Creds   => &self.creds,
            Sort::Fortune => &self.fortune,
        }
    }
}

/// Looks up names/avatars for the given page plus the top entry (thumbnail), skipping
/// users already resolved.
async fn resolve_page(ctx: &serenity::Context, meta: &mut UserMeta, entries: &[Entry], page: usize) {
    let mut missing: Vec<UserId> = Vec::new();
    for (id, ..) in entries.first().into_iter().chain(entries.iter().skip(page * PAGE_SIZE).take(PAGE_SIZE)) {
        if !meta.contains_key(id) && !missing.contains(id) {
            missing.push(*id);
        }
    }
    let fetched = fetch_users(ctx, missing.iter().copied()).await;
    for (id, user) in missing.into_iter().zip(fetched) {
        let avatar = user.as_ref().and_then(serenity::User::avatar_url).unwrap_or_default();
        meta.insert(id, (user_name(user.as_ref()), avatar));
    }
}

async fn render(ctx: &serenity::Context, boards: &Boards, meta: &mut UserMeta, sort: Sort, page: usize) -> serenity::CreateEmbed {
    let entries = boards.get(sort);
    if entries.is_empty() {
        return empty_embed(sort);
    }
    resolve_page(ctx, meta, entries, page).await;
    make_embed(entries, meta, sort, page)
}

/// show server rankings — use buttons to switch between Creds and Fortune
#[poise::command(slash_command)]
pub async fn leaderboard(ctx: Context<'_>) -> Result<(), Error> {
    ctx.defer().await?;
    let serenity_ctx = ctx.serenity_context().clone();

    // Snapshot (UserId, Arc) pairs without holding DashMap shard locks across any await point.
    let user_arcs: Vec<_> = ctx.data().users.iter()
        .map(|e| (*e.key(), Arc::clone(e.value())))
        .collect();

    // Read all user stats concurrently — RwLock reads, no DashMap involvement.
    let stats: Vec<(UserId, i32, i32, String)> =
        futures::future::join_all(user_arcs.iter().map(|(id, u)| async move {
            let u = u.read().await;
            let creds      = u.get_creds();
            let luck_score = u.get_rolling_luck_score();
            let luck_label = u.get_rolling_luck();
            (*id, creds, luck_score, luck_label)
        })).await;

    // Build the sorted leaderboard vectors.
    let mut creds_info:   Vec<Entry> = Vec::new();
    let mut fortune_info: Vec<Entry> = Vec::new();

    for (id, creds, luck_score, luck_label) in stats {
        creds_info.push((id, i64::from(creds), creds.to_string()));

        if luck_score > 0 {
            // Numeric score shown alongside tier so ties within "Blessed" are distinguishable.
            fortune_info.push((id, i64::from(luck_score), format!("{luck_label} ({luck_score})")));
        }
    }

    // Primary: sort key descending. Secondary: user id — deterministic tie-breaking without
    // needing every username up front.
    creds_info.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    fortune_info.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));

    if creds_info.is_empty() {
        ctx.say("No users found.").await?;
        return Ok(());
    }

    let boards = Boards { creds: creds_info, fortune: fortune_info };
    let mut meta = UserMeta::new();

    let mut sort = Sort::Creds;
    let mut page: usize = 0;

    let first = render(&serenity_ctx, &boards, &mut meta, sort, page).await;
    let reply = ctx.send(poise::CreateReply::default()
        .embed(first)
        .components(make_components(sort))
    ).await?;
    let mut msg = reply.into_message().await?;

    // Interaction loop — no spawn, no Arc<RwLock<Message>>, no silent unwrap panics.
    loop {
        let Some(press) = msg
            .await_component_interaction(&serenity_ctx)
            .timeout(Duration::from_secs(60))
            .await
        else {
            // Timeout — grey out the current view and strip buttons.
            let embed = render(&serenity_ctx, &boards, &mut meta, sort, page).await.color(data::EMBED_ERROR);
            msg.edit(&serenity_ctx, EditMessage::default().embed(embed).components(vec![])).await.ok();
            break;
        };

        press.create_response(&serenity_ctx, serenity::CreateInteractionResponse::Acknowledge).await.ok();

        // Compute page bounds from the current sort before applying the new button action.
        let total_pages = page_count(boards.get(sort));

        match press.data.custom_id.as_str() {
            "lb_creds"   => { sort = Sort::Creds;   page = 0; }
            "lb_fortune" => { sort = Sort::Fortune; page = 0; }
            "lb_back"    => { page = page.saturating_sub(1); }
            "lb_next"    => { if page + 1 < total_pages { page += 1; } }
            _            => continue,
        }

        let embed = render(&serenity_ctx, &boards, &mut meta, sort, page).await;
        msg.edit(&serenity_ctx, EditMessage::default().embed(embed).components(make_components(sort))).await.ok();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entries(n: u64) -> Vec<Entry> {
        (1..=n).map(|i| (UserId::new(i), 0, format!("v{i}"))).collect()
    }

    #[test]
    fn page_count_bounds() {
        assert_eq!(page_count(&[]), 1);
        assert_eq!(page_count(&entries(10)), 1);
        assert_eq!(page_count(&entries(11)), 2);
    }

    #[test]
    fn build_page_ranks_slices_and_truncates_names() {
        let list = entries(12);
        let mut meta = UserMeta::new();
        meta.insert(UserId::new(11), ("ジョン・スミス・ジョン・スミス・長い名前".to_string(), String::new()));
        let page2 = build_page(&list, &meta, 1);
        assert!(page2.contains("#11") && page2.contains("#12") && !page2.contains("#10 "));
        assert!(page2.contains("Unknown")); // #12 not resolved
        assert!(page2.contains(&"ジョン・スミス・ジョン・スミス・長い名前".chars().take(16).collect::<String>()));
        assert!(!page2.contains("長い名前"));
        // Past the end: empty table, no panic.
        assert!(!build_page(&list, &meta, 5).contains('#'));
    }
}
