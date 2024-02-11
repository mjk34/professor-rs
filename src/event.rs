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

use crate::data::{self, PokeData, TrainerData};
use crate::serenity;
use crate::{Context, Error};
use rand::seq::SliceRandom;
use rand::{thread_rng, Rng};
use serenity::Color;
use std::vec;

const COMMON: [usize; 61] = [
    1, 4, 7, 10, 11, 13, 14, 16, 19, 20, 21, 23, 27, 29, 32, 35, 39, 41, 43, 46, 47, 48, 50, 51,
    52, 53, 54, 56, 58, 60, 63, 66, 69, 72, 74, 77, 79, 81, 83, 84, 86, 88, 90, 92, 96, 98, 100,
    102, 104, 108, 109, 114, 116, 118, 120, 122, 124, 128, 129, 132, 147,
];
const RARE: [usize; 52] = [
    2, 5, 8, 12, 15, 17, 22, 24, 25, 28, 30, 33, 36, 37, 40, 42, 44, 49, 55, 57, 61, 64, 67, 70,
    73, 75, 78, 80, 82, 85, 87, 89, 91, 93, 95, 97, 99, 101, 103, 105, 110, 111, 117, 119, 121,
    123, 127, 133, 137, 138, 140, 148,
];
const MYTHIC: [usize; 33] = [
    3, 6, 9, 18, 26, 31, 34, 38, 45, 59, 62, 65, 68, 71, 76, 94, 106, 107, 112, 113, 115, 125, 126,
    130, 131, 134, 135, 136, 139, 141, 142, 143, 149,
];
const LEGENDARY: [usize; 5] = [144, 145, 146, 150, 151];

fn get_pokedata(
    ctx: Context<'_>,
    pokemon_name: Option<String>,
    pokemon_index: Option<usize>,
) -> PokeData {
    match (pokemon_name, pokemon_index) {
        // handle for user giving a name
        (Some(pokemon_name), None) => {
            let pokedex = &ctx.data().pokedex;
            let pokemon = if let Some(pkmn) = pokedex
                .iter()
                .find(|&x| x.get_name().to_lowercase() == pokemon_name.to_lowercase())
            {
                pkmn
            } else {
                pokedex
                    .first()
                    .expect("get_pokedata(): Failed to load MissingNo.")
            };

            pokemon.clone()
        }

        // handle for user giving an index
        (None, Some(pokemon_index)) => {
            let pokedex = &ctx.data().pokedex;
            let pokemon = if pokemon_index < 151 {
                pokedex
                    .get(pokemon_index)
                    .expect("get_pokedata(): Failed to load Pokemon from index")
            } else {
                pokedex
                    .first()
                    .expect("get_pokedata(): Failed to load MissingNo.")
            };

            pokemon.clone()
        }

        // handle for user giving both parameters
        (Some(pokemon_name), Some(pokemon_index)) => {
            let pokedex = &ctx.data().pokedex;
            let pokemon = if let Some(pkmn) = pokedex
                .iter()
                .find(|&x| x.get_name().to_lowercase() == pokemon_name.to_lowercase())
            {
                pkmn
            } else if pokemon_index < 151 {
                pokedex
                    .get(pokemon_index)
                    .expect("get_pokedata(): Failed to load Pokemon from index")
            } else {
                pokedex
                    .first()
                    .expect("get_pokedata(): Failed to load MissingNo.")
            };

            pokemon.clone()
        }

        // handle for user giving no parameters
        (None, None) => {
            let pokedex = &ctx.data().pokedex;
            pokedex
                .first()
                .expect("get_pokedata(): Failed to load MissingNo.")
                .clone()
        }
    }
}

/// look up a pokemon by name or by pokedex index entry
#[poise::command(slash_command)]
pub async fn search_pokemon(
    ctx: Context<'_>,
    #[description = "name of the pokemon e.g. bulbasaur, charmander, squirtle..."]
    pokemon_name: Option<String>,
    #[description = "index entry of the pokemon e.g. 1, 2, 3..."] pokemon_index: Option<usize>,
) -> Result<(), Error> {
    let pokemon = get_pokedata(ctx, pokemon_name, pokemon_index);

    let name: String = pokemon.get_name();
    let index: usize = pokemon.get_index();
    let desc: String = pokemon.get_desc();
    let types: String = pokemon.get_types();
    let sprite: String = pokemon.get_sprite();
    let wallpaper: String = pokemon.get_wallpaper();

    let msg_txt = format!("**{}**: {}\n{}", name, types, desc);
    let type_split: Vec<&str> = types.split('/').collect();
    let first_type = type_split
        .first()
        .expect("search_Pokemon(): Failed to expand first_type")
        .to_string();
    let poke_color = get_type_color(&first_type);

    ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title(format!("Pokedex no.{}", index))
                .description(msg_txt)
                .color(Color::new(poke_color))
                .thumbnail(sprite)
                .image(wallpaper)
                .footer(serenity::CreateEmbedFooter::new(
                    "@~ powered by UwUntu & RustyBamboo",
                )),
        ),
    )
    .await?;

    Ok(())
}

#[poise::command(slash_command)]
pub async fn wild_encounter(ctx: Context<'_>) -> Result<(), Error> {
    // TODO: create 3 vectors with specific indexi for common, rare, mythic, legendary pokemon
    //       create persisting message with attack, capture, or run

    let pokemon = spawn_pokemon(ctx);
    let name: String = pokemon.get_name();
    let sprite: String = pokemon.get_sprite();

    let msg_txt = format!("A wild {} appeared!", name);

    let types: String = pokemon.get_types();
    let type_split: Vec<&str> = types.split('/').collect();
    let first_type = type_split
        .first()
        .expect("search_Pokemon(): Failed to expand first_type")
        .to_string();
    let poke_color = get_type_color(&first_type);

    ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title("Wild Encounter")
                .description(msg_txt)
                .color(Color::new(poke_color))
                .image(sprite)
                .footer(serenity::CreateEmbedFooter::new(
                    "@~ powered by UwUntu & RustyBamboo",
                )),
        ),
    )
    .await?;

    Ok(())
}

#[poise::command(slash_command)]
pub async fn trainer_encounter(ctx: Context<'_>, level: u32) -> Result<(), Error> {
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
                .thumbnail("https://cdn.discordapp.com/attachments/1196582162057662484/1206057037247811594/674633.png?ex=65da9f25&is=65c82a25&hm=fca46c5743cea5a20ddbfe7d4d98d2087ac28ade2d6220b88452e2ad3633df88&")
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

fn spawn_pokemon(ctx: Context<'_>) -> PokeData {
    let random_spawn = thread_rng().gen_range(0..100);
    let spawn_index = if random_spawn < 66 {
        COMMON.choose(&mut thread_rng()).unwrap()
    } else if (66..94).contains(&random_spawn) {
        RARE.choose(&mut thread_rng()).unwrap()
    } else if (94..99).contains(&random_spawn) {
        MYTHIC.choose(&mut thread_rng()).unwrap()
    } else {
        LEGENDARY.choose(&mut thread_rng()).unwrap()
    };

    get_pokedata(ctx, None, Some(*spawn_index))
}

fn spawn_trainer(ctx: Context<'_>, level: i32) -> TrainerData {
    // Get Trainer tier based on user level
    let random_trainer = thread_rng().gen_range(0..100);
    let trainer_type = if level < 10 {
        if random_trainer < 90 {
            "Common"
        } else {
            "Mythic"
        }
    } else if (10..20).contains(&level) {
        if random_trainer < 75 {
            "Common"
        } else if (75..98).contains(&random_trainer) {
            "Mythic"
        } else {
            "Legendary"
        }
    } else if (20..30).contains(&level) {
        if random_trainer < 60 {
            "Common"
        } else if (60..95).contains(&random_trainer) {
            "Mythic"
        } else {
            "Legendary"
        }
    } else {
        if random_trainer < 40 {
            "Common"
        } else if (40..80).contains(&random_trainer) {
            "Mythic"
        } else {
            "Legendary"
        }
    };

    // Get random trainer in that tier
    let trainer_list = &ctx.data().trainers;
    let mut trainer: TrainerData = match trainer_type {
        "Mythic" => trainer_list[15..25]
            .choose(&mut thread_rng())
            .unwrap()
            .clone(),
        "Legendary" => trainer_list[25..]
            .choose(&mut thread_rng())
            .unwrap()
            .clone(),
        _ => {
            // Common
            trainer_list[..15]
                .choose(&mut thread_rng())
                .unwrap()
                .clone()
        }
    };

    let mut common_index: Vec<usize> = vec![];
    let mut rare_index: Vec<usize> = vec![];
    let mut mythic_index: Vec<usize> = vec![];
    let mut legendary_index: Vec<usize> = vec![];

    // Sample pokemon from each their based on specific trainer's types
    let trainer_types = trainer.get_types();
    let type_split: Vec<&str> = trainer_types.split('/').collect();
    for typing in type_split {
        for index in COMMON {
            let pokemon = get_pokedata(ctx, None, Some(index));
            if pokemon.get_types().contains(typing) {
                common_index.push(index);
            }
        }
        for index in RARE {
            let pokemon = get_pokedata(ctx, None, Some(index));
            if pokemon.get_types().contains(typing) {
                rare_index.push(index);
            }
        }
        for index in MYTHIC {
            let pokemon = get_pokedata(ctx, None, Some(index));
            if pokemon.get_types().contains(typing) {
                mythic_index.push(index);
            }
        }
        for index in LEGENDARY {
            let pokemon = get_pokedata(ctx, None, Some(index));
            if pokemon.get_types().contains(typing) {
                legendary_index.push(index);
            }
        }
    }

    // Get pokemon based on trainer tier from the sample indexes
    // Common -> 2 pokemon
    // Mythic -> 3 pokemon
    // Legendary -> 4 pokemon
    let team_count = match trainer.get_tier().as_str() {
        "Mythic" => 3,
        "Legendary" => 4,
        _ => 2,
    };

    // Get pokemon, set iconic trainers primary
    let mut team_index: Vec<usize> = vec![];
    for i in 0..team_count {
        // Special trainers
        match (i, trainer.get_name().as_str()) {
            (0, "Ashe") => {
                // pikachu
                team_index.push(25_usize);
                continue;
            }

            (0, "Brock") => {
                // onix
                team_index.push(95_usize);
                continue;
            }

            (0, "Misty") => {
                // starmie
                team_index.push(121_usize);
                continue;
            }

            (0, "Gary") => {
                //eevee
                team_index.push(133_usize);
                continue;
            }

            (0, "Blaine") => {
                // rapidash
                team_index.push(78_usize);
                continue;
            }

            (0, "Giovanni") => {
                // rhydon
                team_index.push(112_usize);
                continue;
            }

            (0, "Erika") => {
                // vileplume
                team_index.push(45_usize);
                continue;
            }

            (0, "Koga") => {
                // venomoth
                team_index.push(49_usize);
                continue;
            }

            (0, "Morty") => {
                // gengar
                team_index.push(95_usize);
                continue;
            }

            (1, "Morty") => {
                // moltres
                team_index.push(146_usize);
                continue;
            }

            (0, "Lance") => {
                //gyrados
                team_index.push(130_usize);
                continue;
            }

            (1, "Lance") => {
                //dragonite
                team_index.push(149_usize);
                continue;
            }

            (0, "Iono") => {
                // zapdos
                team_index.push(25_usize);
                continue;
            }

            (0, "N") => {
                // mew
                team_index.push(151_usize);
                continue;
            }

            (0, "Olivia") => {
                // kabutops
                team_index.push(141_usize);
                continue;
            }

            (0, "Marnie") => {
                // electabuzz
                team_index.push(125_usize);
                continue;
            }

            (0, "Cynthia") => {
                // mewtwo
                team_index.push(150_usize);
                continue;
            }

            (0, "Jesse & James") => {
                // meowth
                team_index.push(52_usize);
                continue;
            }

            _ => {}
        }

        let mut check = true;
        while check {
            let random_tier = thread_rng().gen_range(0..100);
            let poke_index: &usize = match trainer.get_tier().as_str() {
                "Mythic" => {
                    if random_tier < 30 {
                        common_index.choose(&mut thread_rng()).unwrap()
                    } else if (30..80).contains(&random_tier) {
                        rare_index.choose(&mut thread_rng()).unwrap()
                    } else {
                        mythic_index.choose(&mut thread_rng()).unwrap()
                    }
                }
                "Legendary" => {
                    if random_tier < 20 {
                        rare_index.choose(&mut thread_rng()).unwrap()
                    } else if (20..70).contains(&random_tier) {
                        mythic_index.choose(&mut thread_rng()).unwrap()
                    } else {
                        legendary_index.choose(&mut thread_rng()).unwrap()
                    }
                }
                _ => {
                    if random_tier < 70 {
                        common_index.choose(&mut thread_rng()).unwrap()
                    } else {
                        rare_index.choose(&mut thread_rng()).unwrap()
                    }
                }
            };

            // check if pokemon is already in team (no duplicates)
            if !team_index.contains(poke_index) {
                team_index.push(*poke_index);
                check = false;
            }
        }
    }

    team_index.shuffle(&mut thread_rng());
    for index in team_index {
        let mut pokemon = get_pokedata(ctx, None, Some(index));

        // Generate health based on tier
        if LEGENDARY.contains(&index) {
            pokemon.set_health(thread_rng().gen_range(55..65));
        } else if MYTHIC.contains(&index) {
            pokemon.set_health(thread_rng().gen_range(40..50));
        } else if RARE.contains(&index) {
            pokemon.set_health(thread_rng().gen_range(25..35));
        } else {
            pokemon.set_health(thread_rng().gen_range(10..20));
        }

        trainer.give_pokemon(pokemon);
    }

    trainer
}

#[poise::command(slash_command)]
pub async fn test_matchup(
    ctx: Context<'_>,
    #[description = "name of the first pokemon e.g. bulbasaur, charmander, squirtle..."]
    pokemon1: String,
    #[description = "name of the second pokemon e.g. bulbasaur, charmander, squirtle..."]
    pokemon2: String,
) -> Result<(), Error> {
    let pokemon1 = get_pokedata(ctx, Some(pokemon1), None);
    let pokemon2 = get_pokedata(ctx, Some(pokemon2), None);

    let poke1_type = pokemon1.get_types().clone();
    let poke2_type = pokemon2.get_types().clone();

    let poke1_emojis = get_type_emoji(&poke1_type);
    let poke2_emojis = get_type_emoji(&poke2_type);

    let type_advantage = get_advantage(ctx, poke1_type, poke2_type);

    let phrase = if type_advantage >= 2.0 {
        "Super Effective!".to_string()
    } else if type_advantage == 1.0 {
        "Neutral.".to_string()
    } else {
        "Not Very Effective...".to_string()
    };

    let title_txt = format!(
        "{} :crossed_swords: {}",
        pokemon1.get_name(),
        pokemon2.get_name()
    );
    let msg_txt = format!("{} vs {} is **{}**", poke1_emojis, poke2_emojis, phrase);

    ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title(title_txt)
                .description(msg_txt)
                .color(Color::new(16760399))
                .thumbnail("https://archives.bulbagarden.net/media/upload/3/37/RG_Pok%C3%A9dex.png")
                .footer(serenity::CreateEmbedFooter::new(
                    "@~ powered by UwUntu & RustyBamboo",
                )),
        ),
    )
    .await?;

    Ok(())
}

fn get_advantage(ctx: Context<'_>, type1: String, type2: String) -> f32 {
    let matrix = &ctx.data().type_matrix;
    let names = &ctx.data().type_name;
    let dual_type1: Vec<&str> = type1.split('/').collect();
    let dual_type2: Vec<&str> = type2.split('/').collect();

    let mut type_advantage: f32 = 1.0;
    for type1 in &dual_type1 {
        for type2 in &dual_type2 {
            let type1_index = names.iter().position(|x| x == type1).unwrap();
            let type2_index = names.iter().position(|x| x == type2).unwrap();

            let value = matrix.get(type1_index).unwrap().get(type2_index).unwrap();
            type_advantage *= value;
        }
    }

    type_advantage
}

fn get_type_color(typing: &str) -> u32 {
    match typing.to_lowercase().as_str() {
        "normal" => 11053176,
        "fire" => 15761456,
        "water" => 6852848,
        "electric" => 16306224,
        "grass" => 7915600,
        "ice" => 10016984,
        "fighting" => 12595240,
        "poison" => 10502304,
        "ground" => 14729320,
        "flying" => 11047152,
        "psychic" => 16275592,
        "bug" => 11057184,
        "rock" => 12099640,
        "ghost" => 7362712,
        "dark" => 7362632,
        "steel" => 12105936,
        "fairy" => 15775420,
        "dragon" => 7354616,
        _ => 2039583,
    }
}

fn get_type_emoji(typing: &str) -> String {
    let dual_type: Vec<&str> = typing.split('/').collect();
    let mut emojis: String = "".to_string();
    for element in dual_type {
        emojis += match element.to_lowercase().as_str() {
            "normal" => ":white_circle:",
            "fire" => ":fire:",
            "water" => ":droplet:",
            "electric" => ":zap:",
            "grass" => ":leaves:",
            "ice" => ":snowflake:",
            "fighting" => ":punch:",
            "poison" => ":skull:",
            "ground" => ":mountain:",
            "flying" => ":wing:",
            "psychic" => ":fish_cake:",
            "bug" => ":lady_beetle:",
            "rock" => ":ring:",
            "ghost" => ":ghost:",
            "dark" => ":waxing_crescent_moon:",
            "steel" => "nut_and_bolt:",
            "fairy" => ":fairy:",
            "dragon" => ":dragon:",
            _ => "",
        }
    }

    emojis
}
