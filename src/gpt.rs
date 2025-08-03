//!---------------------------------------------------------------------!
//! This file contains a collection of chat gpt related that is         !
//! to professorBot's chat completion and dalli doodles                 !
//!                                                                     !
//! Functions:                                                          !
//!     [x] - gpt_string                                                !
//!     [x] - gpt_doodle                                                !
//!---------------------------------------------------------------------!

use openai_api_rs::v1::api::{OpenAIClient, OpenAIClientBuilder};
use openai_api_rs::v1::chat_completion::{self, ChatCompletionRequest};
use openai_api_rs::v1::common::DALL_E_3;
use openai_api_rs::v1::common::GPT4_1106_PREVIEW;
use openai_api_rs::v1::error::APIError;
use openai_api_rs::v1::image::ImageGenerationRequest;
use rand::{thread_rng, Rng};
use std::env;

/// These APIs no longer have the adaquate number of tokens per day to do fortunes...
/// use gpt-3.5-turbo to generate fun responses to user prompts
pub async fn gpt_string(api_key: String, prompt: String) -> Result<String, APIError> {
    let client = OpenAIClientBuilder::new()
        .with_api_key(api_key.to_string())
        .build()
        .unwrap();

    let req = ChatCompletionRequest::new(
        GPT4_1106_PREVIEW.to_string(),
        vec![chat_completion::ChatCompletionMessage {
            role: chat_completion::MessageRole::user,
            content: chat_completion::Content::Text(prompt),
            name: None,
            tool_calls: None,
            tool_call_id: None,
        }],
    );

    let result = client.chat_completion(req).await?;
    let desc = format!(
        "{:?}",
        result.choices[0]
            .message
            .content
            .as_ref()
            .unwrap()
            .to_string()
    );

    Ok(desc.replace(['\"', '\\'], ""))
}

// use dalli-e-3 to generate fun doodles in various styles
pub async fn gpt_doodle(api_key: String, prompt: String) -> Result<String, APIError> {
    let client = OpenAIClientBuilder::new()
        .with_api_key(api_key.to_string())
        .build()
        .unwrap();

    let req = ImageGenerationRequest::new(prompt).model(DALL_E_3.to_string());
    let result = client.image_generation(req).await?;

    Ok(result.data.first().unwrap().url.to_string())
}

pub async fn generate_doodle(messages: &mut [String], randomstyle: bool) -> String {
    let gpt_key: String = env::var("API_KEY").expect("missing GPT API_KEY");

    messages.reverse();
    let full_message_history = messages.join("\n");
    let style = thread_rng().gen_range(0.0..=1.0);

    println!("randomstyle?: {:?}", randomstyle);

    let prompt = if randomstyle {
        if style < 0.50 {
            "simple small doodle of a ".to_owned() + &full_message_history
        } else {
            "cute pixelated art of a ".to_owned() + &full_message_history
        }
    } else {
        full_message_history
    };

    match gpt_doodle(gpt_key, prompt.clone()).await {
        Ok(doodle_url) => doodle_url,
        Err(_) => "https://cdn.discordapp.com/attachments/1260223476766343188/1278857715891830875/collapsed-stair-structure-of-wooden-cubes-with-upward-pointing-arrows-business-risk-due-to.webp?ex=66d2548f&is=66d1030f&hm=8265ac5a5caf779f6415b8ae2e71843b78ff80d73ef1870df4150ae99991d785&".to_string(),
    }
}

pub async fn generate_text(messages: &mut [String]) -> String {
    let gpt_key: String = env::var("API_KEY").expect("missing GPT API_KEY");

    messages.reverse();
    let full_message_history = messages.join("\n");
    let prompt = if thread_rng().gen::<f64>() < 0.8 {
        full_message_history + "(make it simple and answer in a cute tone with minimal uwu emojis like a tsundere, dont write big paragraphs, keep it short)"
    } else {
        full_message_history + "(make it simple and answer in a cute tone with murderous emojis like a yandere, dont write big paragraphs, keep it short)"
    };

    let mut tries = 0;
    let mut reading;
    loop {
        match gpt_string(gpt_key.clone(), prompt.clone()).await {
            Ok(result) => {
                reading = result;
                break;
            }
            Err(e) => {
                println!("An error occurred: {:?}, retrying...", e);
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                if tries > 3 {
                    return "Professor failed chat completion... please try again later."
                        .to_string();
                }
            }
        }
        tries += 1;
    }

    reading = reading.replace("nn", "\n");
    reading
}
