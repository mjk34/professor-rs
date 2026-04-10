//! Buy and sell modal definitions used by the /search command.

use crate::serenity;

pub fn parse_trade_fields(data: &serenity::ModalInteractionData) -> (String, String, String) {
    let mut portfolio   = String::new();
    let mut amount      = String::new();
    let mut limit_price = String::new();
    for row in &data.components {
        for comp in &row.components {
            if let serenity::ActionRowComponent::InputText(t) = comp {
                match t.custom_id.as_str() {
                    "portfolio"   => portfolio   = t.value.clone().unwrap_or_default(),
                    "amount"      => amount      = t.value.clone().unwrap_or_default(),
                    "limit_price" => limit_price = t.value.clone().unwrap_or_default(),
                    _ => {}
                }
            }
        }
    }
    (portfolio, amount, limit_price)
}

#[derive(Debug)]
pub struct BuyModal {
    pub portfolio: String,
    pub amount: String,
    pub limit_price: String,
    /// Per-portfolio cash breakdown shown in the read-only display field.
    pub portfolio_info: String,
}

impl poise::Modal for BuyModal {
    fn create(defaults: Option<Self>, custom_id: String) -> serenity::CreateInteractionResponse {
        let portfolio_val  = defaults.as_ref().map_or("", |d| d.portfolio.as_str());
        let amount_val     = defaults.as_ref().map_or("", |d| d.amount.as_str());
        let portfolio_info = defaults.as_ref().map_or("", |d| d.portfolio_info.as_str());

        let mut components = vec![
            serenity::CreateActionRow::InputText(
                serenity::CreateInputText::new(
                    serenity::InputTextStyle::Short, "Portfolio", "portfolio",
                ).value(portfolio_val)
            ),
        ];

        if !portfolio_info.is_empty() {
            components.push(serenity::CreateActionRow::InputText(
                serenity::CreateInputText::new(
                    serenity::InputTextStyle::Paragraph, "Available Cash", "portfolio_info",
                )
                .value(portfolio_info)
                .required(false)
            ));
        }

        components.push(serenity::CreateActionRow::InputText(
            serenity::CreateInputText::new(
                serenity::InputTextStyle::Short, "Amount (shares e.g. 10, or dollars e.g. $500)", "amount",
            )
            .placeholder("e.g. 10 or $500")
            .value(amount_val)
        ));

        components.push(serenity::CreateActionRow::InputText(
            serenity::CreateInputText::new(
                serenity::InputTextStyle::Short, "Limit Price (optional)", "limit_price",
            )
            .placeholder("e.g. 150.00 — buy when price ≤ this (blank = market)")
            .required(false)
        ));

        serenity::CreateInteractionResponse::Modal(
            serenity::CreateModal::new(custom_id, "Buy").components(components)
        )
    }

    fn parse(data: serenity::ModalInteractionData) -> Result<Self, &'static str> {
        let (portfolio, amount, limit_price) = parse_trade_fields(&data);
        Ok(Self { portfolio, amount, limit_price, portfolio_info: String::new() })
    }
}

#[derive(Debug)]
pub struct SellModal {
    pub portfolio: String,
    pub amount: String,
    pub limit_price: String,
    /// Dynamic label injected into the Amount field (e.g. "10.5 shares ($1,234.56)").
    pub holdings_info: String,
}

impl poise::Modal for SellModal {
    fn create(defaults: Option<Self>, custom_id: String) -> serenity::CreateInteractionResponse {
        let portfolio_val = defaults.as_ref().map_or("", |d| d.portfolio.as_str());
        let amount_val    = defaults.as_ref().map_or("", |d| d.amount.as_str());
        let amount_label  = defaults.as_ref().and_then(|d| {
            if d.holdings_info.is_empty() { None }
            else {
                let s = format!("Amount — {}", d.holdings_info);
                Some(s.chars().take(45).collect::<String>())
            }
        }).unwrap_or_else(|| "Amount (shares, dollars, or 'all')".to_string());

        serenity::CreateInteractionResponse::Modal(
            serenity::CreateModal::new(custom_id, "Sell")
                .components(vec![
                    serenity::CreateActionRow::InputText(
                        serenity::CreateInputText::new(
                            serenity::InputTextStyle::Short, "Portfolio", "portfolio",
                        ).value(portfolio_val)
                    ),
                    serenity::CreateActionRow::InputText(
                        serenity::CreateInputText::new(
                            serenity::InputTextStyle::Short, amount_label, "amount",
                        )
                        .placeholder("e.g. 10, $500, 50%, or all")
                        .value(amount_val)
                    ),
                    serenity::CreateActionRow::InputText(
                        serenity::CreateInputText::new(
                            serenity::InputTextStyle::Short, "Limit Price (optional)", "limit_price",
                        )
                        .placeholder("e.g. 200.00 — sell when price ≥ this (blank = market)")
                        .required(false)
                    ),
                ])
        )
    }

    fn parse(data: serenity::ModalInteractionData) -> Result<Self, &'static str> {
        let (portfolio, amount, limit_price) = parse_trade_fields(&data);
        Ok(Self { portfolio, amount, limit_price, holdings_info: String::new() })
    }
}
