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

use crate::data::PokeData;
use crate::serenity;
use crate::{Context, Error};
use rand::seq::SliceRandom;
use rand::{thread_rng, Rng};
use serenity::Color;
use std::vec;

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

    let pokemon = spawn_random(ctx);
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

fn spawn_random(ctx: Context<'_>) -> PokeData {
    let common: [usize; 61] = [
        1, 4, 7, 10, 11, 13, 14, 16, 19, 20, 21, 23, 27, 29, 32, 35, 39, 41, 43, 46, 47, 48, 50,
        51, 52, 53, 54, 56, 58, 60, 63, 66, 69, 72, 74, 77, 79, 81, 83, 84, 86, 88, 90, 92, 96, 98,
        100, 102, 104, 108, 109, 114, 116, 118, 120, 122, 124, 128, 129, 132, 147,
    ];
    let rare: [usize; 52] = [
        2, 5, 8, 12, 15, 17, 22, 24, 25, 28, 30, 33, 36, 37, 40, 42, 44, 49, 55, 57, 61, 64, 67,
        70, 73, 75, 78, 80, 82, 85, 87, 89, 91, 93, 95, 97, 99, 101, 103, 105, 110, 111, 117, 119,
        121, 123, 127, 133, 137, 138, 140, 148,
    ];
    let mythic: [usize; 33] = [
        3, 6, 9, 18, 26, 31, 34, 38, 45, 59, 62, 65, 68, 71, 76, 94, 106, 107, 112, 113, 115, 125,
        126, 130, 131, 134, 135, 136, 139, 141, 142, 143, 149,
    ];
    let legendary: [usize; 5] = [144, 145, 146, 150, 151];

    let random_spawn = thread_rng().gen_range(0..100);
    let spawn_index = if random_spawn < 66 {
        common.choose(&mut thread_rng()).unwrap()
    } else if (66..94).contains(&random_spawn) {
        rare.choose(&mut thread_rng()).unwrap()
    } else if (94..99).contains(&random_spawn) {
        mythic.choose(&mut thread_rng()).unwrap()
    } else {
        legendary.choose(&mut thread_rng()).unwrap()
    };

    get_pokedata(ctx, None, Some(*spawn_index))
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
