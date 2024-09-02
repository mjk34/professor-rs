//!---------------------------------------------------------------------!
//! This file contains a collection of EVENT related commands, the      !
//! current event is POKEMON themed!                                    !
//!                                                                     !
//! Commands:                                                           !
//!     [x] - get_pokedata                                              !
//!     [x] - search_pokemon                                            !
//!     [x] - test_matchup                                              !
//!     [-] - pokedex                                                   !
//!     [ ] - wild_encounter                                            !
//!     [ ] - trainer_battle                                            !
//!---------------------------------------------------------------------!

use crate::data::{self, PokeData};
use crate::poke_helper::{
    generate_btns, generate_hp, get_advantage, get_type_color, spawn_pokemon, spawn_trainer,
    COMMON, MYTHIC, RARE,
};
use crate::serenity;
use crate::{Context, Error};
use poise::serenity_prelude::futures::StreamExt;
use poise::serenity_prelude::EditMessage;
use rand::{thread_rng, Rng};
use serenity::Color;
use std::sync::Arc;
use std::time::Duration;
use std::vec;
use tokio::sync::RwLock;
use tokio::time::sleep;

#[poise::command(slash_command)]
pub async fn wild_encounter(ctx: Context<'_>) -> Result<(), Error> {
    let user = ctx.author();
    let data = &ctx.data().users;
    let u = data.get(&user.id).unwrap();
    let user_data = u.read().await;

    let level = user_data.get_level();
    let type_matrix = ctx.data().type_matrix.clone();
    let type_name = ctx.data().type_name.clone();

    // Wild pokemon object
    let mut wild_pokemon = spawn_pokemon(ctx, level);
    wild_pokemon.set_health(generate_hp(wild_pokemon.get_index()));

    // Wild pokemon information
    let wild_pokemon_types: String = wild_pokemon.get_types().clone();
    let wild_pokemon_color = get_type_color(&wild_pokemon_types);

    // Player pokemon team and first pokemon (buddy) object
    let mut current = user_data.event.get_buddy();
    let mut player_team = user_data.event.get_team().clone();
    let mut player_pokemon = player_team.get(current).unwrap().clone();

    // Player pokemon information
    let mut player_pokemon_types: String = player_pokemon.get_types().clone();
    let mut player_pokemon_color = get_type_color(&player_pokemon_types);

    let continue_btn = serenity::CreateButton::new("open_modal")
        .label("Continue")
        .custom_id("continue".to_string())
        .style(poise::serenity_prelude::ButtonStyle::Success);

    let components = vec![serenity::CreateActionRow::Buttons(vec![continue_btn])];

    let reply = ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title("Wild Pokemon")
                .description(format!("A wild **{}** appeared!", &wild_pokemon.get_name()))
                .color(Color::new(wild_pokemon_color))
                // TODO update this link with cool wild pokemon encounter gif idk
                .thumbnail("https://cdn.discordapp.com/attachments/1260223476766343188/1262229534716072026/Untitled-1.png?ex=6695d65c&is=669484dc&hm=9737a4d44e5184c8dbe99e2b23e6dccf98fb4c48abd48a1a017801f8852ffb8e&")
                .image(&wild_pokemon.get_wallpaper())
                .footer(serenity::CreateEmbedFooter::new(
                    "@~ powered by UwUntu & RustyBamboo",
                )),
        ).components(components)
    )
    .await?;

    let msg_og = Arc::new(RwLock::new(reply.into_message().await?));
    let msg = Arc::clone(&msg_og);
    let mut reactions = msg
        .read()
        .await
        .await_component_interactions(ctx)
        .timeout(Duration::new(120, 0))
        .stream();

    let ctx = ctx.serenity_context().clone();

    let user_id = user.id;
    let user_avatar = user.avatar_url().unwrap_or_default().to_string();
    let u = Arc::clone(&u);

    tokio::spawn(async move {
        let mut timeout_check = true;
        let mut catch_success = false;
        let mut defeat_pokemon = false;
        let mut wild_go_first = false;
        let mut forced_switch = false;

        async fn timeout_exit(
            msg: &std::sync::Arc<tokio::sync::RwLock<poise::serenity_prelude::Message>>,
            ctx: &poise::serenity_prelude::Context,
            user_avatar: &String,
        ) {
            let desc = "[Response timed out... try again tomorrow (next: `/uwu`)]".to_string();
            msg.write()
                .await
                .edit(
                    ctx,
                    EditMessage::default()
                        .embed(
                            serenity::CreateEmbed::default()
                                .title("????????".to_string())
                                .description(&desc)
                                .thumbnail(user_avatar)
                                .colour(data::EMBED_ERROR)
                                .footer(serenity::CreateEmbedFooter::new(
                                    "@~ powered by UwUntu & RustyBamboo",
                                )),
                        )
                        .components(Vec::new()),
                )
                .await
                .unwrap();
        }

        async fn wild_pokemeon_turn(
            msg: &std::sync::Arc<tokio::sync::RwLock<poise::serenity_prelude::Message>>,
            ctx: &poise::serenity_prelude::Context,
            u: &Arc<RwLock<data::UserData>>,
            wild_pokemon: &mut PokeData,
            player_pokemon: &mut PokeData,
            current: &mut usize,
            wild_multiplier: f32,
        ) -> bool {
            let wild_attack_roll = if COMMON.contains(&wild_pokemon.get_index()) {
                thread_rng().gen_range(2..8)
            } else if RARE.contains(&wild_pokemon.get_index()) {
                thread_rng().gen_range(3..10)
            } else if MYTHIC.contains(&wild_pokemon.get_index()) {
                thread_rng().gen_range(4..12)
            } else {
                thread_rng().gen_range(5..14)
            };

            let wild_pokemon_color = get_type_color(&wild_pokemon.get_types());
            let random_idle = thread_rng().gen_range(0..100);
            let mut forced_switch = false;

            let mut desc = "﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n\n".to_string();

            desc += format!(
                " \u{3000} \u{3000}Wild **{}**'s turn... \n",
                &wild_pokemon.get_name()
            )
            .as_str();

            desc += format!(
                "\n\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n**{}** |  HP: {}/{}",
                &player_pokemon.get_name(),
                &player_pokemon.get_current_health(),
                &player_pokemon.get_health()
            )
            .as_str();

            msg.write()
                .await
                .edit(
                    &ctx,
                    EditMessage::default().embed(
                        serenity::CreateEmbed::default()
                            .title(format!(
                                "Wild **{}** |  HP: {}/{}",
                                &wild_pokemon.get_name(),
                                &wild_pokemon.get_current_health(),
                                &wild_pokemon.get_health()
                            ))
                            .description(desc)
                            .thumbnail(&wild_pokemon.get_sprite())
                            .image(&player_pokemon.get_bsprite())
                            .colour(wild_pokemon_color)
                            .footer(serenity::CreateEmbedFooter::new(
                                "@~ powered by UwUntu & RustyBamboo",
                            )),
                    ),
                )
                .await
                .unwrap();

            sleep(Duration::from_millis(1400)).await;

            if random_idle < 12 {
                let mut desc = "﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n\n".to_string();

                desc += format!(
                    " \u{3000} \u{3000}Wild **{}** dazed into the horizon... \n",
                    &wild_pokemon.get_name()
                )
                .as_str();

                desc += format!(
                    "\n\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n**{}** |  HP: {}/{}",
                    &player_pokemon.get_name(),
                    &player_pokemon.get_current_health(),
                    &player_pokemon.get_health()
                )
                .as_str();

                msg.write()
                    .await
                    .edit(
                        &ctx,
                        EditMessage::default().embed(
                            serenity::CreateEmbed::default()
                                .title(format!(
                                    "Wild **{}** |  HP: {}/{}",
                                    &wild_pokemon.get_name(),
                                    &wild_pokemon.get_current_health(),
                                    &wild_pokemon.get_health()
                                ))
                                .description(desc)
                                .thumbnail(&wild_pokemon.get_sprite())
                                .image(&player_pokemon.get_bsprite())
                                .colour(wild_pokemon_color)
                                .footer(serenity::CreateEmbedFooter::new(
                                    "@~ powered by UwUntu & RustyBamboo",
                                )),
                        ),
                    )
                    .await
                    .unwrap();

                return forced_switch;
            }

            let mut desc = "﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n\n".to_string();

            desc += format!(
                " \u{3000} \u{3000}Wild **{}** rolling to attack... \n",
                &wild_pokemon.get_name()
            )
            .as_str();

            desc += format!(
                "\n\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n**{}** |  HP: {}/{}",
                &player_pokemon.get_name(),
                &player_pokemon.get_current_health(),
                &player_pokemon.get_health()
            )
            .as_str();

            msg.write()
                .await
                .edit(
                    &ctx,
                    EditMessage::default().embed(
                        serenity::CreateEmbed::default()
                            .title(format!(
                                "Wild **{}** |  HP: {}/{}",
                                &wild_pokemon.get_name(),
                                &wild_pokemon.get_current_health(),
                                &wild_pokemon.get_health()
                            ))
                            .description(desc)
                            .thumbnail(&wild_pokemon.get_sprite())
                            .image(&player_pokemon.get_bsprite())
                            .colour(wild_pokemon_color)
                            .footer(serenity::CreateEmbedFooter::new(
                                "@~ powered by UwUntu & RustyBamboo",
                            )),
                    ),
                )
                .await
                .unwrap();

            sleep(Duration::from_millis(700)).await;

            let mut desc = "﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n\n".to_string();

            desc += format!(
                " \u{3000} \u{3000}Wild **{}** rolling to attack... \n",
                &wild_pokemon.get_name()
            )
            .as_str();

            desc += format!(
                " \u{3000} \u{3000}Wild **{}** rolled a... . . . **{}**\n",
                &wild_pokemon.get_name(),
                wild_attack_roll
            )
            .as_str();

            desc += format!(
                "\n\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n**{}** |  HP: {}/{}",
                &player_pokemon.get_name(),
                &player_pokemon.get_current_health(),
                &player_pokemon.get_health()
            )
            .as_str();

            msg.write()
                .await
                .edit(
                    &ctx,
                    EditMessage::default().embed(
                        serenity::CreateEmbed::default()
                            .title(format!(
                                "Wild **{}** |  HP: {}/{}",
                                &wild_pokemon.get_name(),
                                &wild_pokemon.get_current_health(),
                                &wild_pokemon.get_health()
                            ))
                            .description(desc)
                            .thumbnail(&wild_pokemon.get_sprite())
                            .image(&player_pokemon.get_bsprite())
                            .colour(wild_pokemon_color)
                            .footer(serenity::CreateEmbedFooter::new(
                                "@~ powered by UwUntu & RustyBamboo",
                            )),
                    ),
                )
                .await
                .unwrap();

            sleep(Duration::from_millis(700)).await;

            let wild_damage = (wild_attack_roll as f32 * wild_multiplier).round() as i32;
            if player_pokemon.get_current_health() - wild_damage <= 0 {
                let _ = &player_pokemon.set_current_health(0);

                forced_switch = true;

                let mut user_data = u.write().await;
                user_data.event.faint_pokemon(*current);
            } else {
                _ = &player_pokemon
                    .set_current_health(player_pokemon.get_current_health() - wild_damage);

                let mut user_data = u.write().await;
                user_data.event.take_damage(*current, wild_damage);
            }

            let mut desc = "﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n\n".to_string();

            desc += format!(
                " \u{3000} \u{3000}Wild **{}** rolling to attack... \n",
                wild_pokemon.get_name()
            )
            .as_str();

            desc += format!(
                " \u{3000} \u{3000}Wild **{}** rolled a... . . . **{}**\n\n",
                wild_pokemon.get_name(),
                wild_attack_roll
            )
            .as_str();

            desc += format!(
                " \u{3000} \u{3000}Wild **{}** attacks for **{}** damage!\n\n",
                wild_pokemon.get_name(),
                wild_damage
            )
            .as_str();

            if wild_multiplier >= 2.0 {
                desc += " \u{3000} \u{3000} **Super Effective**!!";
            } else if (1.0..2.0).contains(&wild_multiplier) {
                desc += " \u{3000} \u{3000} Effective.";
            } else {
                desc += " \u{3000} \u{3000} *Not very Effective*...";
            }

            desc += format!(
                "\n\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n**{}** |  HP: {}/{}",
                &player_pokemon.get_name(),
                &player_pokemon.get_current_health(),
                &player_pokemon.get_health()
            )
            .as_str();

            msg.write()
                .await
                .edit(
                    &ctx,
                    EditMessage::default().embed(
                        serenity::CreateEmbed::default()
                            .title(format!(
                                "Wild **{}** |  HP: {}/{}",
                                &wild_pokemon.get_name(),
                                &wild_pokemon.get_current_health(),
                                &wild_pokemon.get_health()
                            ))
                            .description(desc)
                            .thumbnail(&wild_pokemon.get_sprite())
                            .image(&player_pokemon.get_bsprite())
                            .colour(wild_pokemon_color)
                            .footer(serenity::CreateEmbedFooter::new(
                                "@~ powered by UwUntu & RustyBamboo",
                            )),
                    ),
                )
                .await
                .unwrap();

            forced_switch
        }

        async fn show_team(
            msg: &std::sync::Arc<tokio::sync::RwLock<poise::serenity_prelude::Message>>,
            ctx: &poise::serenity_prelude::Context,
            user_avatar: &String,
            player_team: &[PokeData],
            player_pokemon_name: &String,
            current: &usize,
            forced: bool,
        ) -> bool {
            let mut desc = "\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n".to_string();
            let mut buttons = Vec::new();

            if !forced {
                let back_btn = serenity::CreateButton::new("open_modal")
                    .label("back")
                    .custom_id("back".to_string())
                    .style(poise::serenity_prelude::ButtonStyle::Secondary);
                buttons.push(back_btn);
            }

            let mut available_switch = 0;
            for (index, pokemon) in player_team.iter().enumerate() {
                let name = pokemon.get_name();
                let types = pokemon.get_types();
                let current_hp = pokemon.get_current_health();
                let health = pokemon.get_health();

                if index == *current && forced {
                    if name.len() < 7 {
                        desc +=
                            format!("- {}. ~~{:22}~~  \u{2000}\u{2000}", index + 1, name).as_str();
                    } else {
                        desc += format!("- {}. ~~{:20}~~  ", index + 1, name).as_str();
                    }

                    desc += format!("{:20}  HP: {}/{}\n", types, current_hp, health).as_str();
                } else if index == *current && !forced {
                    if name.len() < 7 {
                        desc +=
                            format!("- {}. **{:22}**  \u{2000}\u{2000}", index + 1, name).as_str();
                    } else {
                        desc += format!("- {}. **{:20}**  ", index + 1, name).as_str();
                    }

                    desc += format!("{:20}  HP: {}/{}\n", types, current_hp, health).as_str();
                } else {
                    if name.len() < 7 {
                        desc += format!("- {}. {:22}  \u{2000}", index + 1, name).as_str();
                    } else {
                        desc += format!("- {}. {:20}  ", index + 1, name).as_str();
                    }

                    desc += format!("{:20}  HP: {}/{}\n", types, current_hp, health).as_str();

                    let pk_btn = serenity::CreateButton::new("open_modal")
                        .label(&name)
                        .custom_id(format!("pokemon-{}", index))
                        .style(poise::serenity_prelude::ButtonStyle::Primary);
                    buttons.push(pk_btn);

                    available_switch += 1;
                }
            }

            if available_switch <= 0 {
                msg.write()
                    .await
                    .edit(
                        &ctx,
                        EditMessage::default()
                            .embed(
                                serenity::CreateEmbed::default()
                                    .title("Battle Lost...".to_string())
                                    .thumbnail(user_avatar)
                                    .image("https://c.tenor.com/IyFy4R9syeMAAAAC/tenor.gif")
                                    .colour(data::EMBED_ERROR)
                                    .footer(serenity::CreateEmbedFooter::new(
                                        "@~ powered by UwUntu & RustyBamboo",
                                    )),
                            )
                            .components(Vec::new()),
                    )
                    .await
                    .unwrap();
                return true;
            }

            let components = vec![serenity::CreateActionRow::Buttons(buttons)];

            desc += "\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n";

            let title_text = if forced {
                format!(
                    "{} fainted... which pokemon do you want to send out?",
                    player_pokemon_name
                )
            } else {
                "Switch Pokemon?".to_string()
            };

            msg.write()
                .await
                .edit(
                    &ctx,
                    EditMessage::default()
                        .embed(
                            serenity::CreateEmbed::default()
                                .title(title_text)
                                .description(desc)
                                .thumbnail("https://cdn.discordapp.com/attachments/1260223476766343188/1262210407167168603/pokeballs.png?ex=6695c48b&is=6694730b&hm=cd6c20793501c3a6bf2dc1ffcafd79606a2b3381920a5ffa89efb125d595ceb7&")
                                .colour(data::EMBED_DEFAULT)
                                .footer(serenity::CreateEmbedFooter::new(
                                    "@~ powered by UwUntu & RustyBamboo",
                                )),
                        )
                        .components(components),
                )
                .await
                .unwrap();
            false
        }

        while let Some(reaction) = reactions.next().await {
            reaction
                .create_response(&ctx, serenity::CreateInteractionResponse::Acknowledge)
                .await
                .unwrap();

            let react_id = reaction.member.clone().unwrap_or_default().user.id;
            if react_id == user_id && reaction.data.custom_id.as_str() == "continue" {
                let wild_roll = thread_rng().gen_range(1..21);
                let player_roll = thread_rng().gen_range(1..21);

                let mut desc = "﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n\n".to_string();

                desc += " \u{3000} \u{3000} \u{3000}Rolling for initiative... \n";

                desc += format!(
                    "\n\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n**{}** |  HP: {}/{}",
                    &player_pokemon.get_name(),
                    &player_pokemon.get_current_health(),
                    &player_pokemon.get_health()
                )
                .as_str();

                msg.write()
                    .await
                    .edit(
                        &ctx,
                        EditMessage::default()
                            .embed(
                                serenity::CreateEmbed::default()
                                    .title(format!(
                                        "Wild **{}** |  HP: {}/{}",
                                        &wild_pokemon.get_name(),
                                        &wild_pokemon.get_current_health(),
                                        &wild_pokemon.get_health()
                                    ))
                                    .description(desc)
                                    .thumbnail(&wild_pokemon.get_sprite())
                                    .image(&player_pokemon.get_bsprite())
                                    .colour(data::EMBED_DEFAULT)
                                    .footer(serenity::CreateEmbedFooter::new(
                                        "@~ powered by UwUntu & RustyBamboo",
                                    )),
                            )
                            .components(Vec::new()),
                    )
                    .await
                    .unwrap();

                sleep(Duration::from_millis(700)).await;

                let mut desc = "﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n\n".to_string();

                desc += " \u{3000} \u{3000} \u{3000}Rolling for initiative... \n";

                desc += format!(
                    " \u{3000} \u{3000} \u{3000}Wild **{}** rolled a... . . . **{}**\n",
                    wild_pokemon.get_name(),
                    wild_roll
                )
                .as_str();

                desc += format!(
                    "\n\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n**{}** |  HP: {}/{}",
                    &player_pokemon.get_name(),
                    &player_pokemon.get_current_health(),
                    &player_pokemon.get_health()
                )
                .as_str();

                msg.write()
                    .await
                    .edit(
                        &ctx,
                        EditMessage::default().embed(
                            serenity::CreateEmbed::default()
                                .title(format!(
                                    "Wild **{}** |  HP: {}/{}",
                                    &wild_pokemon.get_name(),
                                    &wild_pokemon.get_current_health(),
                                    &wild_pokemon.get_health()
                                ))
                                .description(desc)
                                .thumbnail(&wild_pokemon.get_sprite())
                                .image(&player_pokemon.get_bsprite())
                                .colour(data::EMBED_DEFAULT)
                                .footer(serenity::CreateEmbedFooter::new(
                                    "@~ powered by UwUntu & RustyBamboo",
                                )),
                        ),
                    )
                    .await
                    .unwrap();

                sleep(Duration::from_millis(700)).await;

                let mut desc = "﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n\n".to_string();

                desc += " \u{3000} \u{3000} \u{3000}Rolling for initiative... \n";

                desc += format!(
                    " \u{3000} \u{3000} \u{3000}Wild **{}** rolled a... . . . **{}**\n",
                    wild_pokemon.get_name(),
                    wild_roll
                )
                .as_str();

                desc += format!(
                    " \u{3000} \u{3000} \u{3000}**{}** rolled a... . . . **{}**\n\n",
                    &player_pokemon.get_name(),
                    player_roll
                )
                .as_str();

                desc += format!(
                    "\n\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n**{}** |  HP: {}/{}",
                    &player_pokemon.get_name(),
                    &player_pokemon.get_current_health(),
                    &player_pokemon.get_health()
                )
                .as_str();

                msg.write()
                    .await
                    .edit(
                        &ctx,
                        EditMessage::default().embed(
                            serenity::CreateEmbed::default()
                                .title(format!(
                                    "Wild **{}** |  HP: {}/{}",
                                    &wild_pokemon.get_name(),
                                    &wild_pokemon.get_current_health(),
                                    &wild_pokemon.get_health()
                                ))
                                .description(desc)
                                .thumbnail(&wild_pokemon.get_sprite())
                                .image(&player_pokemon.get_bsprite())
                                .colour(data::EMBED_DEFAULT)
                                .footer(serenity::CreateEmbedFooter::new(
                                    "@~ powered by UwUntu & RustyBamboo",
                                )),
                        ),
                    )
                    .await
                    .unwrap();

                sleep(Duration::from_millis(700)).await;

                let mut desc = "﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n\n".to_string();

                desc += " \u{3000} \u{3000} \u{3000}Rolling for initiative... \n";

                desc += format!(
                    " \u{3000} \u{3000} \u{3000}Wild **{}** rolled a... . . . **{}**\n",
                    &wild_pokemon.get_name(),
                    wild_roll
                )
                .as_str();

                desc += format!(
                    " \u{3000} \u{3000} \u{3000}**{}** rolled a... . . . **{}**\n\n",
                    &player_pokemon.get_name(),
                    player_roll
                )
                .as_str();

                let tmp_color: u32;
                if player_roll > wild_roll || player_roll == 1 && wild_roll == 1 {
                    desc += format!(
                        " \u{3000} \u{3000} \u{3000}**{}** goes first!\n",
                        &player_pokemon.get_name()
                    )
                    .as_str();

                    tmp_color = player_pokemon_color;
                } else {
                    desc += format!(
                        " \u{3000} \u{3000} \u{3000}Wild **{}** goes first!\n",
                        &wild_pokemon.get_name()
                    )
                    .as_str();

                    wild_go_first = true;
                    tmp_color = wild_pokemon_color;
                }

                desc += format!(
                    "\n\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n**{}** |  HP: {}/{}",
                    &player_pokemon.get_name(),
                    &player_pokemon.get_current_health(),
                    &player_pokemon.get_health()
                )
                .as_str();

                msg.write()
                    .await
                    .edit(
                        &ctx,
                        EditMessage::default().embed(
                            serenity::CreateEmbed::default()
                                .title(format!(
                                    "Wild **{}** |  HP: {}/{}",
                                    &wild_pokemon.get_name(),
                                    &wild_pokemon.get_current_health(),
                                    &wild_pokemon.get_health()
                                ))
                                .description(desc)
                                .thumbnail(&wild_pokemon.get_sprite())
                                .image(&player_pokemon.get_bsprite())
                                .colour(tmp_color)
                                .footer(serenity::CreateEmbedFooter::new(
                                    "@~ powered by UwUntu & RustyBamboo",
                                )),
                        ),
                    )
                    .await
                    .unwrap();

                timeout_check = false;
                break;
            }
        }

        if timeout_check {
            timeout_exit(&msg, &ctx, &user_avatar).await;
            return;
        } else {
            if wild_go_first {
                sleep(Duration::from_millis(1000)).await;

                // potentiall return if forced switch
                let wild_multiplier = get_advantage(
                    &type_matrix,
                    &type_name,
                    &wild_pokemon_types,
                    &player_pokemon_types,
                );
                forced_switch = wild_pokemeon_turn(
                    &msg,
                    &ctx,
                    &u,
                    &mut wild_pokemon,
                    &mut player_pokemon,
                    &mut current,
                    wild_multiplier,
                )
                .await;

                if forced_switch {
                    let user_data = u.read().await;
                    player_team = user_data.event.get_team().clone();
                }

                // create force switch here to make playr switch, if user has no team left, battle finishes,
                // player looses creds and buddy goes back to 1 hp
                if forced_switch {
                    sleep(Duration::from_millis(1000)).await;

                    let lost = show_team(
                        &msg,
                        &ctx,
                        &user_avatar,
                        &player_team,
                        &player_pokemon.get_name(),
                        &current,
                        forced_switch,
                    )
                    .await;

                    if lost {
                        return;
                    }

                    while let Some(reaction) = reactions.next().await {
                        reaction
                            .create_response(&ctx, serenity::CreateInteractionResponse::Acknowledge)
                            .await
                            .unwrap();

                        let react_id = reaction.member.clone().unwrap_or_default().user.id;
                        let mut new_buddy = current;
                        if react_id == user_id {
                            new_buddy = match reaction.data.custom_id.as_str() {
                                "pokemon-0" => 0,
                                "pokemon-1" => 1,
                                "pokemon-2" => 2,
                                "pokemon-3" => 3,
                                "pokemon-4" => 4,
                                _ => current,
                            };

                            // let mut user_data = u.write().await;
                            // user_data.event.set_buddy(new_buddy);

                            timeout_check = false;
                        }

                        if new_buddy != current {
                            let user_data = u.read().await;
                            // current = user_data.event.get_buddy();
                            current = new_buddy;
                            player_team = user_data.event.get_team().clone();
                            player_pokemon = player_team.get(current).unwrap().clone();
                            player_pokemon_types = player_pokemon.get_types().clone();
                            player_pokemon_color = get_type_color(&player_pokemon_types);

                            let mut desc =
                                "﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n\n".to_string();

                            desc +=
                                format!(" \u{3000} \u{3000}Go {}! \n", &player_pokemon.get_name())
                                    .as_str();
                            desc += format!(
                                "\n\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n**{}** |  HP: {}/{}",
                                &player_pokemon.get_name(),
                                &player_pokemon.get_current_health(),
                                &player_pokemon.get_health()
                            )
                            .as_str();

                            msg.write()
                                .await
                                .edit(
                                    &ctx,
                                    EditMessage::default()
                                        .embed(
                                            serenity::CreateEmbed::default()
                                                .title(format!(
                                                    "Wild **{}** |  HP: {}/{}",
                                                    &wild_pokemon.get_name(),
                                                    &wild_pokemon.get_current_health(),
                                                    &wild_pokemon.get_health()
                                                ))
                                                .description(desc)
                                                .thumbnail(&wild_pokemon.get_sprite())
                                                .image(&player_pokemon.get_bsprite())
                                                .colour(data::EMBED_DEFAULT)
                                                .footer(serenity::CreateEmbedFooter::new(
                                                    "@~ powered by UwUntu & RustyBamboo",
                                                )),
                                        )
                                        .components(Vec::new()),
                                )
                                .await
                                .unwrap();

                            timeout_check = false;
                            break;
                        }
                    }

                    if timeout_check {
                        timeout_exit(&msg, &ctx, &user_avatar).await;
                        return;
                    }
                }
            }

            sleep(Duration::from_millis(1700)).await;

            let mut desc = "﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n\n".to_string();

            desc += " \u{3000} \u{3000}Your turn... \n";
            desc += format!(
                "\n\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n**{}** |  HP: {}/{}",
                &player_pokemon.get_name(),
                &player_pokemon.get_current_health(),
                &player_pokemon.get_health()
            )
            .as_str();

            let components = generate_btns();
            msg.write()
                .await
                .edit(
                    &ctx,
                    EditMessage::default()
                        .embed(
                            serenity::CreateEmbed::default()
                                .title(format!(
                                    "Wild **{}** |  HP: {}/{}",
                                    &wild_pokemon.get_name(),
                                    &wild_pokemon.get_current_health(),
                                    &wild_pokemon.get_health()
                                ))
                                .description(desc)
                                .thumbnail(&wild_pokemon.get_sprite())
                                .image(&player_pokemon.get_bsprite())
                                .colour(data::EMBED_DEFAULT)
                                .footer(serenity::CreateEmbedFooter::new(
                                    "@~ powered by UwUntu & RustyBamboo",
                                )),
                        )
                        .components(components),
                )
                .await
                .unwrap();
        }

        // Create a check for who goes first ---- roll for initiative

        while !catch_success && !defeat_pokemon {
            let mut player_switch = false;
            let mut switch_cancel = false;
            // let mut  player_bag = false;

            forced_switch = false;
            timeout_check = true;

            while let Some(reaction) = reactions.next().await {
                reaction
                    .create_response(&ctx, serenity::CreateInteractionResponse::Acknowledge)
                    .await
                    .unwrap();

                let react_id = reaction.member.clone().unwrap_or_default().user.id;
                if react_id == user_id && reaction.data.custom_id.as_str() == "fight" {
                    let attack_roll = if COMMON.contains(&player_pokemon.get_index()) {
                        thread_rng().gen_range(2..8)
                    } else if RARE.contains(&player_pokemon.get_index()) {
                        thread_rng().gen_range(3..10)
                    } else if MYTHIC.contains(&player_pokemon.get_index()) {
                        thread_rng().gen_range(4..12)
                    } else {
                        thread_rng().gen_range(5..14)
                    };

                    let multiplier = get_advantage(
                        &type_matrix,
                        &type_name,
                        &player_pokemon_types,
                        &wild_pokemon_types,
                    );

                    let mut desc = "﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n\n".to_string();

                    desc += format!(
                        " \u{3000} \u{3000}**{}** rolling to attack... \n",
                        player_pokemon.get_name()
                    )
                    .as_str();

                    desc += format!(
                        "\n\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n**{}** |  HP: {}/{}",
                        &player_pokemon.get_name(),
                        &player_pokemon.get_current_health(),
                        &player_pokemon.get_health()
                    )
                    .as_str();

                    msg.write()
                        .await
                        .edit(
                            &ctx,
                            EditMessage::default()
                                .embed(
                                    serenity::CreateEmbed::default()
                                        .title(format!(
                                            "Wild **{}** |  HP: {}/{}",
                                            &wild_pokemon.get_name(),
                                            &wild_pokemon.get_current_health(),
                                            &wild_pokemon.get_health()
                                        ))
                                        .description(desc)
                                        .thumbnail(&wild_pokemon.get_sprite())
                                        .image(&player_pokemon.get_bsprite())
                                        .colour(player_pokemon_color)
                                        .footer(serenity::CreateEmbedFooter::new(
                                            "@~ powered by UwUntu & RustyBamboo",
                                        )),
                                )
                                .components(Vec::new()),
                        )
                        .await
                        .unwrap();

                    sleep(Duration::from_millis(700)).await;

                    let mut desc = "﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n\n".to_string();

                    desc += format!(
                        " \u{3000} \u{3000}**{}** rolling to attack... \n",
                        &player_pokemon.get_name()
                    )
                    .as_str();

                    desc += format!(
                        " \u{3000} \u{3000}**{}** rolled a... . . . **{}**\n",
                        &player_pokemon.get_name(),
                        attack_roll
                    )
                    .as_str();

                    desc += format!(
                        "\n\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n**{}** |  HP: {}/{}",
                        &player_pokemon.get_name(),
                        &player_pokemon.get_current_health(),
                        &player_pokemon.get_health()
                    )
                    .as_str();

                    msg.write()
                        .await
                        .edit(
                            &ctx,
                            EditMessage::default().embed(
                                serenity::CreateEmbed::default()
                                    .title(format!(
                                        "Wild **{}** |  HP: {}/{}",
                                        &wild_pokemon.get_name(),
                                        &wild_pokemon.get_current_health(),
                                        &wild_pokemon.get_health()
                                    ))
                                    .description(desc)
                                    .thumbnail(&wild_pokemon.get_sprite())
                                    .image(&player_pokemon.get_bsprite())
                                    .colour(player_pokemon_color)
                                    .footer(serenity::CreateEmbedFooter::new(
                                        "@~ powered by UwUntu & RustyBamboo",
                                    )),
                            ),
                        )
                        .await
                        .unwrap();

                    sleep(Duration::from_millis(700)).await;

                    let damage = (attack_roll as f32 * multiplier).round() as i32;
                    if wild_pokemon.get_current_health() - damage < 0 {
                        wild_pokemon.set_current_health(0);
                    } else {
                        wild_pokemon.set_current_health(wild_pokemon.get_current_health() - damage);
                    }

                    let mut desc = "﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n\n".to_string();

                    desc += format!(
                        " \u{3000} \u{3000}**{}** rolling to attack... \n",
                        &player_pokemon.get_name()
                    )
                    .as_str();

                    desc += format!(
                        " \u{3000} \u{3000}**{}** rolled a... . . . **{}**\n\n",
                        &player_pokemon.get_name(),
                        attack_roll
                    )
                    .as_str();

                    desc += format!(
                        " \u{3000} \u{3000}**{}** attacks for **{}** damage!\n\n",
                        &player_pokemon.get_name(),
                        damage
                    )
                    .as_str();

                    if multiplier >= 2.0 {
                        desc += " \u{3000} \u{3000} **Super Effective**!!";
                    } else if (1.0..2.0).contains(&multiplier) {
                        desc += " \u{3000} \u{3000} Effective.";
                    } else {
                        desc += " \u{3000} \u{3000} *Not very Effective*...";
                    }

                    desc += format!(
                        "\n\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n**{}** |  HP: {}/{}",
                        &player_pokemon.get_name(),
                        &player_pokemon.get_current_health(),
                        &player_pokemon.get_health()
                    )
                    .as_str();

                    msg.write()
                        .await
                        .edit(
                            &ctx,
                            EditMessage::default().embed(
                                serenity::CreateEmbed::default()
                                    .title(format!(
                                        "Wild **{}** |  HP: {}/{}",
                                        &wild_pokemon.get_name(),
                                        &wild_pokemon.get_current_health(),
                                        &wild_pokemon.get_health()
                                    ))
                                    .description(desc)
                                    .thumbnail(&wild_pokemon.get_sprite())
                                    .image(&player_pokemon.get_bsprite())
                                    .colour(player_pokemon_color)
                                    .footer(serenity::CreateEmbedFooter::new(
                                        "@~ powered by UwUntu & RustyBamboo",
                                    )),
                            ),
                        )
                        .await
                        .unwrap();

                    timeout_check = false;
                    break;
                }

                // provide list of pokemon and buttons to switch pokemon
                if react_id == user_id && reaction.data.custom_id.as_str() == "switch" {
                    player_switch = true;
                    break;
                }

                // create a bag menu that shows potions, pokeballs and berries
                if react_id == user_id && reaction.data.custom_id.as_str() == "bag" {
                    msg.write()
                        .await
                        .edit(
                            &ctx,
                            EditMessage::default()
                                .embed(
                                    serenity::CreateEmbed::default()
                                        .title("Bag is not yet implemented! (EXITING)".to_string())
                                        .thumbnail(&user_avatar)
                                        .colour(data::EMBED_ERROR)
                                        .footer(serenity::CreateEmbedFooter::new(
                                            "@~ powered by UwUntu & RustyBamboo",
                                        )),
                                )
                                .components(Vec::new()),
                        )
                        .await
                        .unwrap();

                    return;
                    // timeout_check = false;
                    // break;
                }
            }

            if player_switch {
                let lost = show_team(
                    &msg,
                    &ctx,
                    &user_avatar,
                    &player_team,
                    &player_pokemon.get_name(),
                    &current,
                    forced_switch,
                )
                .await;

                if lost {
                    return;
                }

                while let Some(reaction) = reactions.next().await {
                    reaction
                        .create_response(&ctx, serenity::CreateInteractionResponse::Acknowledge)
                        .await
                        .unwrap();

                    let react_id = reaction.member.clone().unwrap_or_default().user.id;
                    if react_id == user_id && reaction.data.custom_id.as_str() == "back" {
                        let mut desc = "﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n\n".to_string();

                        desc += " \u{3000} \u{3000}Your turn... \n";
                        desc += format!(
                            "\n\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n**{}** |  HP: {}/{}",
                            &player_pokemon.get_name(),
                            &player_pokemon.get_current_health(),
                            &player_pokemon.get_health()
                        )
                        .as_str();

                        let components = generate_btns();
                        msg.write()
                            .await
                            .edit(
                                &ctx,
                                EditMessage::default()
                                    .embed(
                                        serenity::CreateEmbed::default()
                                            .title(format!(
                                                "Wild **{}** |  HP: {}/{}",
                                                &wild_pokemon.get_name(),
                                                &wild_pokemon.get_current_health(),
                                                &wild_pokemon.get_health()
                                            ))
                                            .description(desc)
                                            .thumbnail(&wild_pokemon.get_sprite())
                                            .image(&player_pokemon.get_bsprite())
                                            .colour(data::EMBED_DEFAULT)
                                            .footer(serenity::CreateEmbedFooter::new(
                                                "@~ powered by UwUntu & RustyBamboo",
                                            )),
                                    )
                                    .components(components),
                            )
                            .await
                            .unwrap();

                        timeout_check = false;
                        switch_cancel = true;
                        break;
                    }

                    let mut new_buddy = current;
                    if react_id == user_id {
                        new_buddy = match reaction.data.custom_id.as_str() {
                            "pokemon-0" => 0,
                            "pokemon-1" => 1,
                            "pokemon-2" => 2,
                            "pokemon-3" => 3,
                            "pokemon-4" => 4,
                            _ => current,
                        };

                        // let mut user_data = u.write().await;
                        // user_data.event.set_buddy(new_buddy);

                        switch_cancel = false;
                        timeout_check = false;
                    }

                    if new_buddy != current {
                        let user_data = u.read().await;
                        // current = user_data.event.get_buddy();
                        current = new_buddy;
                        player_team = user_data.event.get_team().clone();
                        player_pokemon = player_team.get(current).unwrap().clone();
                        player_pokemon_types = player_pokemon.get_types().clone();
                        player_pokemon_color = get_type_color(&player_pokemon_types);

                        let mut desc = "﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n\n".to_string();

                        desc += format!(" \u{3000} \u{3000}Go {}! \n", &player_pokemon.get_name())
                            .as_str();
                        desc += format!(
                            "\n\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n**{}** |  HP: {}/{}",
                            &player_pokemon.get_name(),
                            &player_pokemon.get_current_health(),
                            &player_pokemon.get_health()
                        )
                        .as_str();

                        msg.write()
                            .await
                            .edit(
                                &ctx,
                                EditMessage::default()
                                    .embed(
                                        serenity::CreateEmbed::default()
                                            .title(format!(
                                                "Wild **{}** |  HP: {}/{}",
                                                &wild_pokemon.get_name(),
                                                &wild_pokemon.get_current_health(),
                                                &wild_pokemon.get_health()
                                            ))
                                            .description(desc)
                                            .thumbnail(&wild_pokemon.get_sprite())
                                            .image(&player_pokemon.get_bsprite())
                                            .colour(data::EMBED_DEFAULT)
                                            .footer(serenity::CreateEmbedFooter::new(
                                                "@~ powered by UwUntu & RustyBamboo",
                                            )),
                                    )
                                    .components(Vec::new()),
                            )
                            .await
                            .unwrap();

                        timeout_check = false;
                        switch_cancel = false;
                        break;
                    }
                }

                if timeout_check {
                    timeout_exit(&msg, &ctx, &user_avatar).await;
                    return;
                }
            }

            // if player_bag {}

            if !switch_cancel && !timeout_check && wild_pokemon.get_current_health() == 0 {
                defeat_pokemon = true;
            }

            if !switch_cancel && !timeout_check && !defeat_pokemon && !catch_success {
                sleep(Duration::from_millis(1400)).await;

                // potentiall return if forced switch
                let wild_multiplier = get_advantage(
                    &type_matrix,
                    &type_name,
                    &wild_pokemon_types,
                    &player_pokemon_types,
                );
                forced_switch = wild_pokemeon_turn(
                    &msg,
                    &ctx,
                    &u,
                    &mut wild_pokemon,
                    &mut player_pokemon,
                    &mut current,
                    wild_multiplier,
                )
                .await;

                if forced_switch {
                    let user_data = u.read().await;
                    player_team = user_data.event.get_team().clone();
                }

                // create force switch here to make playr switch, if user has no team left, battle finishes,
                // player looses creds and buddy goes back to 1 hp
                if forced_switch {
                    let lost = show_team(
                        &msg,
                        &ctx,
                        &user_avatar,
                        &player_team,
                        &player_pokemon.get_name(),
                        &current,
                        forced_switch,
                    )
                    .await;

                    if lost {
                        return;
                    }

                    while let Some(reaction) = reactions.next().await {
                        reaction
                            .create_response(&ctx, serenity::CreateInteractionResponse::Acknowledge)
                            .await
                            .unwrap();

                        let react_id = reaction.member.clone().unwrap_or_default().user.id;
                        let mut new_buddy = current;
                        if react_id == user_id {
                            new_buddy = match reaction.data.custom_id.as_str() {
                                "pokemon-0" => 0,
                                "pokemon-1" => 1,
                                "pokemon-2" => 2,
                                "pokemon-3" => 3,
                                "pokemon-4" => 4,
                                _ => current,
                            };

                            // let mut user_data = u.write().await;
                            // user_data.event.set_buddy(new_buddy);

                            timeout_check = false;
                        }

                        if new_buddy != current {
                            let user_data = u.read().await;
                            // current = user_data.event.get_buddy();
                            current = new_buddy;
                            player_team = user_data.event.get_team().clone();
                            player_pokemon = player_team.get(current).unwrap().clone();
                            player_pokemon_types = player_pokemon.get_types().clone();
                            player_pokemon_color = get_type_color(&player_pokemon_types);

                            let mut desc =
                                "﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n\n".to_string();

                            desc +=
                                format!(" \u{3000} \u{3000}Go {}! \n", &player_pokemon.get_name())
                                    .as_str();
                            desc += format!(
                                "\n\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n**{}** |  HP: {}/{}",
                                &player_pokemon.get_name(),
                                &player_pokemon.get_current_health(),
                                &player_pokemon.get_health()
                            )
                            .as_str();

                            msg.write()
                                .await
                                .edit(
                                    &ctx,
                                    EditMessage::default()
                                        .embed(
                                            serenity::CreateEmbed::default()
                                                .title(format!(
                                                    "Wild **{}** |  HP: {}/{}",
                                                    &wild_pokemon.get_name(),
                                                    &wild_pokemon.get_current_health(),
                                                    &wild_pokemon.get_health()
                                                ))
                                                .description(desc)
                                                .thumbnail(&wild_pokemon.get_sprite())
                                                .image(&player_pokemon.get_bsprite())
                                                .colour(data::EMBED_DEFAULT)
                                                .footer(serenity::CreateEmbedFooter::new(
                                                    "@~ powered by UwUntu & RustyBamboo",
                                                )),
                                        )
                                        .components(Vec::new()),
                                )
                                .await
                                .unwrap();

                            timeout_check = false;
                            break;
                        }
                    }

                    if timeout_check {
                        timeout_exit(&msg, &ctx, &user_avatar).await;
                        return;
                    }
                }

                sleep(Duration::from_millis(1700)).await;

                let mut desc = "﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n\n".to_string();

                desc += " \u{3000} \u{3000}Your turn... \n";
                desc += format!(
                    "\n\n﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋﹋\n**{}** |  HP: {}/{}",
                    &player_pokemon.get_name(),
                    &player_pokemon.get_current_health(),
                    &player_pokemon.get_health()
                )
                .as_str();

                let components = generate_btns();
                msg.write()
                    .await
                    .edit(
                        &ctx,
                        EditMessage::default()
                            .embed(
                                serenity::CreateEmbed::default()
                                    .title(format!(
                                        "Wild **{}** |  HP: {}/{}",
                                        &wild_pokemon.get_name(),
                                        &wild_pokemon.get_current_health(),
                                        &wild_pokemon.get_health()
                                    ))
                                    .description(desc)
                                    .thumbnail(&wild_pokemon.get_sprite())
                                    .image(&player_pokemon.get_bsprite())
                                    .colour(data::EMBED_DEFAULT)
                                    .footer(serenity::CreateEmbedFooter::new(
                                        "@~ powered by UwUntu & RustyBamboo",
                                    )),
                            )
                            .components(components),
                    )
                    .await
                    .unwrap();
            }

            if timeout_check {
                timeout_exit(&msg, &ctx, &user_avatar).await;
                return;
            }
        }

        msg.write()
            .await
            .edit(
                &ctx,
                EditMessage::default()
                    .embed(
                        serenity::CreateEmbed::default()
                            .title("Battle Won!".to_string())
                            .thumbnail(&user_avatar)
                            .image("https://c.tenor.com/KvxhuFxIHuoAAAAd/tenor.gif")
                            .colour(data::EMBED_CYAN)
                            .footer(serenity::CreateEmbedFooter::new(
                                "@~ powered by UwUntu & RustyBamboo",
                            )),
                    )
                    .components(Vec::new()),
            )
            .await
            .unwrap();
    });

    Ok(())
}

#[poise::command(slash_command)]
pub async fn trainer_battle(ctx: Context<'_>, level: u32) -> Result<(), Error> {
    let trainer = spawn_trainer(ctx, level as i32);

    let msg_txt: String = match trainer.get_tier().as_str() {
        "Mythic" => {
            format!(
                "Notorious Trainer **{}** wants to test your skills!",
                trainer.get_name()
            )
        }
        "Legendary" => {
            format!(
                "International Trainer **{}** invites you to battle!",
                trainer.get_name()
            )
        }
        _ => {
            format!(
                "**{}** is looking for a pokemon battle!",
                trainer.get_name()
            )
        }
    };
    let trainer_img = trainer.get_wallpaper();

    ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title("Trainer Battle")
                .description(msg_txt)
                .color(data::EMBED_TRAINER)
                .image(trainer_img)
                .thumbnail("https://cdn.discordapp.com/attachments/1260223476766343188/1262229974534983731/674633.png?ex=6695d6c5&is=66948545&hm=3149f5b144d45452af48159b54aa81f38c7a2d39b9d0ab60c03701cf323aa821&")
                .footer(serenity::CreateEmbedFooter::new(
                    "@~ powered by UwUntu & RustyBamboo",
                )),
        ),
    )
    .await?;

    for pokemon in trainer.get_team() {
        let name = pokemon.get_name();
        let types: String = pokemon.get_types();
        let type_split: Vec<&str> = types.split('/').collect();
        let first_type = type_split
            .first()
            .expect("search_Pokemon(): Failed to expand first_type")
            .to_string();
        let poke_color = get_type_color(&first_type);

        let desc = format!(
            "HP: {}/{}",
            pokemon.get_current_health(),
            pokemon.get_health()
        );

        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title(format!("{}'s {}", trainer.get_name(), name))
                    .description(desc)
                    .thumbnail(pokemon.get_sprite())
                    .color(Color::new(poke_color))
                    .footer(serenity::CreateEmbedFooter::new(
                        "@~ powered by UwUntu & RustyBamboo",
                    )),
            ),
        )
        .await?;
    }

    Ok(())
}
