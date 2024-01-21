
use crate::serenity;
use crate::{Context, Error};
use serenity::Color;

#[poise::command(slash_command)]
pub async fn search_pokemon(ctx: Context<'_>, pokedex_no: usize) -> Result<(), Error> {
    if pokedex_no > 151 {
        let msg_txt = format!("Entry {} does not exist.", pokedex_no);

        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Pokedex no.---")
                    .description(msg_txt)
                    .color(Color::new(16760399))
                    .thumbnail("https://archives.bulbagarden.net/media/upload/3/37/RG_Pok%C3%A9dex.png")
                    .footer(serenity::CreateEmbedFooter::new(
                        "@~ powered by UwUntu & RustyBamboo",
                    )),
            ),
        )
        .await?;

    } else {
        let pokemon = ctx
            .data()
            .pokedex
            .get(pokedex_no)
            .expect(format!("Could not find Pokemon no.{}", pokedex_no).as_str());
        let name: String = pokemon.get_name();
        let desc: String = pokemon.get_desc();
        let types: String = pokemon.get_types();
        let sprite: String = pokemon.get_sprite();

        let msg_txt = format!("**{}**: {}\n{}", name, types, desc);
        let type_split: Vec<&str> = types.split("/").collect();
        let first_type = type_split.get(0).expect("Failed to expand type").to_string();
        let poke_color = get_type_color(&first_type);

        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title(format!("Pokedex no.{}", pokedex_no))
                    .description(msg_txt)
                    .color(Color::new(poke_color))
                    .thumbnail(sprite)
                    .footer(serenity::CreateEmbedFooter::new(
                        "@~ powered by UwUntu & RustyBamboo",
                    )),
            ),
        )
        .await?;
    }

    Ok(())
}

#[poise::command(slash_command)]
pub async fn test_matchup(ctx: Context<'_>, poke1: usize, poke2: usize)  -> Result<(), Error> {

    if poke1 > 151 || poke2 > 151 {
        let msg_txt = if poke1 > 152 {
            format!("Entry {} does not exist.", poke1)
        } else {
            format!("Entry {} does not exist.", poke2)
        };

        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Pokedex no.---")
                    .description(msg_txt)
                    .color(Color::new(16760399))
                    .thumbnail("https://archives.bulbagarden.net/media/upload/3/37/RG_Pok%C3%A9dex.png")
                    .footer(serenity::CreateEmbedFooter::new(
                        "@~ powered by UwUntu & RustyBamboo",
                    )),
            ),
        )
        .await?;

    } else {
        let pokemon1 = ctx
            .data()
            .pokedex
            .get(poke1)
            .expect(format!("Could not find Pokemon no.{}", poke1).as_str());
            
        let pokemon2 = ctx
            .data()
            .pokedex
            .get(poke2)
            .expect(format!("Could not find Pokemon no.{}", poke2).as_str());

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
        
        let title_txt = format!("{} :crossed_swords: {}", pokemon1.get_name(), pokemon2.get_name());
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
    }  

    Ok(())
}

fn get_advantage (ctx: Context<'_>, type1: String, type2: String) -> f32{

    let matrix = &ctx.data().type_matrix;
    let names = &ctx.data().type_name;
    let dual_type1: Vec<&str> = type1.split("/").collect();
    let dual_type2: Vec<&str> = type2.split("/").collect();
        
    let mut type_advantage: f32 = 1.0;
    for type1 in &dual_type1 {
        for type2 in &dual_type2 {
            let type1_index = names.iter().position(|x| x == type1).unwrap();
            let type2_index = names.iter().position(|x| x == type2).unwrap();

            let value = matrix.get(type1_index).unwrap().get(type2_index).unwrap();
            type_advantage *= value.clone();
        }
    }

        return type_advantage;
}

fn get_type_color (typing: &String) -> u32 {
    return match typing.to_lowercase().as_str() {
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
        _ => 2039583
    }

}

fn get_type_emoji (typing: &String) -> String {
    let dual_type: Vec<&str> = typing.split("/").collect();
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
            _ => ""
        }
    }

    return emojis;
}