//! `/options_quote` command

use super::engine::{parse_expiry, ERR_EXPIRY_PAST, ERR_INVALID_EXPIRY, SHARES_PER_CONTRACT, TIME_VALUE_PER_DTE};
use crate::api::{fetch_price, market_data_err};
use crate::data::{self, OptionType};
use crate::{serenity, Context, Error};
use crate::helper::{default_footer, option_intrinsic, option_type_str, price_to_creds};
use chrono::Utc;

/// Get the intrinsic value of an options contract
#[poise::command(slash_command)]
pub async fn options_quote(
    ctx: Context<'_>,
    #[description = "Underlying ticker (e.g. AAPL)"] ticker: String,
    #[description = "Strike price in USD"] strike: f64,
    #[description = "Expiry date (YYYY-MM-DD)"] expiry: String,
    #[description = "Call or Put"] option_type: OptionType,
) -> Result<(), Error> {
    let opt_type = option_type;

    let expiry_dt = if let Some(d) = parse_expiry(&expiry) { d } else {
        ctx.send(poise::CreateReply::default().embed(
            serenity::CreateEmbed::new().title("Options Quote").description(ERR_INVALID_EXPIRY).color(data::EMBED_ERROR),
        )).await?;
        return Ok(());
    };

    if expiry_dt < Utc::now() {
        ctx.send(poise::CreateReply::default().embed(
            serenity::CreateEmbed::new().title("Options Quote").description(ERR_EXPIRY_PAST).color(data::EMBED_ERROR),
        )).await?;
        return Ok(());
    }

    ctx.defer().await?;
    let ticker = ticker.to_uppercase();
    let price_usd = if let Some(p) = fetch_price(&ticker).await { p } else {
        ctx.send(poise::CreateReply::default().embed(
            serenity::CreateEmbed::new().title("Options Quote").description(market_data_err(&ticker)).color(data::EMBED_ERROR),
        )).await?;
        return Ok(());
    };

    let intrinsic = option_intrinsic(opt_type, price_usd, strike);
    let dte = (expiry_dt - Utc::now()).num_days().max(0);
    let time_value_usd = dte as f64 * TIME_VALUE_PER_DTE;
    let premium_per_contract_usd = (intrinsic + time_value_usd).max(0.01) * SHARES_PER_CONTRACT;
    let premium_creds = price_to_creds(premium_per_contract_usd);
    let itm = intrinsic > 0.0;
    let type_str = option_type_str(opt_type);

    ctx.send(poise::CreateReply::default().embed(
        serenity::CreateEmbed::new()
            .title("Options Quote")
            .description(format!(
                "**{} {} ${:.2}** exp {} ({} DTE)\n\nUnderlying: **${:.2}**\nIntrinsic: **${:.2}/contract** | Time value: **${:.2}/contract**\nPremium: **${:.2}/contract** ({:.0} creds)\nStatus: **{}**",
                ticker, type_str, strike, expiry, dte,
                price_usd,
                intrinsic * SHARES_PER_CONTRACT, time_value_usd * SHARES_PER_CONTRACT,
                premium_per_contract_usd, premium_creds,
                if itm { "In The Money (ITM)" } else { "Out of The Money (OTM)" }
            ))
            .color(if itm { data::EMBED_SUCCESS } else { data::EMBED_ERROR })
            .footer(default_footer()),
    )).await?;
    Ok(())
}
