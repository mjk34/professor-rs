use rand::seq::SliceRandom;
use rand::{thread_rng, Rng};

use crate::serenity;
use crate::{Context, Error};
use serenity::Color;

// Ping the bot to see if its alive or to play ping pong
#[poise::command(prefix_command, slash_command)]
pub async fn ping(ctx: Context<'_>) -> Result<(), Error> {
    let author = ctx.author();
    let pong_image = ctx.data().pong.choose(&mut thread_rng()).unwrap();
    ctx.send(
        poise::CreateReply::default()
            // .content("Pong!")
            .embed(
                serenity::CreateEmbed::new()
                    .title("Pong!")
                    .description(format!("{}", author.name))
                    .image(pong_image),
            ),
    )
    .await?;

    Ok(())
}

// Use gpt-3.5-turbo to generate fun responses to user prompts
#[poise::command(prefix_command, slash_command, track_edits)]
pub async fn gpt_string(_ctx: Context<'_>) -> Result<(), Error> {
    Ok(())
}

#[poise::command(slash_command)]
pub async fn uwu(ctx: Context<'_>) -> Result<(), Error> {
    let user = ctx.author();
    let mut data = ctx.data().users.lock().await;
    let user_data = data.get_mut(&user.id).unwrap();

    let ponder_image = ctx.data().ponder.choose(&mut thread_rng()).unwrap();

    //TODO: match original
    let num = thread_rng().gen_range(0..100);

    if !user_data.check_daily() {
        ctx.send(
            poise::CreateReply::default().embed(
                serenity::CreateEmbed::new()
                    .title("Daily")
                    .description("Your next **/uwu** is tomorrow")
                    .thumbnail(format!("{}", user.avatar_url().unwrap_or_default())),
            ),
        )
        .await?;
        return Ok(());
    }
    user_data.add_creds(num);
    user_data.update_daily();

    let pog_str = if num > 70 {
        "Super Pog!"
    } else if num > 50 {
        "Pog!"
    } else {
        "Sadge..."
    };

    let desc = format!("**{} +{}** added to your Wallet!", pog_str, num);

    ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title("Daily")
                .description(desc)
                .thumbnail(format!("{}", user.avatar_url().unwrap_or_default()))
                .color(Color::GOLD)
                .image(ponder_image),
        ),
    )
    .await?;

    Ok(())
}

#[poise::command(slash_command)]
pub async fn wallet(ctx: Context<'_>) -> Result<(), Error> {
    let user = ctx.author();
    let data = ctx.data().users.lock().await;
    let user_data = data.get(&user.id).unwrap();

    let desc = format!("Total Creds: **{}**", user_data.get_creds());

    ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title("Wallet")
                .description(desc)
                .thumbnail(format!("{}", user.avatar_url().unwrap_or_default()))
                .color(Color::GOLD),
        ),
    )
    .await?;

    Ok(())
}

#[poise::command(slash_command)]
pub async fn voice_status(ctx: Context<'_>) -> Result<(), Error> {
    let data = ctx.data().voice_users.lock().await;

    let mut out: Vec<_> = data.iter().collect();
    out.sort_by(|a, b| a.1.joined.cmp(&b.1.joined));

    let now = chrono::Utc::now();
    let mut desc = "".to_string();

    if out.len() > 0 {
        for (a, b) in out.iter() {
            let u = a.to_user(&ctx).await?;
            let diff = now - b.joined;
            let minutes = ((diff.num_seconds()) / 60) % 60;
            let hours = (diff.num_seconds() / 60) / 60;
            desc += &format!("**{}**: {:0>2}:{:0>2}", u.name, hours, minutes);

            if let Some(mute_time) = b.mute {
                let mute_duration = now - mute_time;
                let mute_minutes = ((mute_duration.num_seconds()) / 60) % 60;
                let mute_hours = (mute_duration.num_seconds() / 60) / 60;
                desc += &format!(" | Mute: {:0>2}:{:0>2}", mute_hours, mute_minutes);
            }
            if let Some(deaf_time) = b.deaf {
                let deaf_duration = now - deaf_time;
                let deaf_minutes = ((deaf_duration.num_seconds()) / 60) % 60;
                let deaf_hours = (deaf_duration.num_seconds() / 60) / 60;
                desc += &format!(" | Deaf: {:0>2}:{:0>2}", deaf_hours, deaf_minutes);
            }
            desc += "\n";
        }
    } else {
        desc = "No one in voice".to_string();
    }

    ctx.send(
        poise::CreateReply::default().embed(
            serenity::CreateEmbed::new()
                .title("Voice Status")
                .description(desc)
                .color(Color::GOLD),
        ),
    )
    .await?;

    Ok(())
}
