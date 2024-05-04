use ollama_rs::generation::completion::request::GenerationRequest;
use openai_api_rs::v1::api::Client;
use openai_api_rs::v1::chat_completion::{self, ChatCompletionRequest};
use openai_api_rs::v1::common::GPT3_5_TURBO_16K;

pub(crate) trait LLM {
    async fn gpt_string(&self, prompt: String) -> Result<String, ()>;
}

struct OpenAI {
    client: Client,
}

impl OpenAI {
    fn new(api_key: String) -> Self {
        let client = Client::new(api_key.to_string());
        return OpenAI { client };
    }
}

impl LLM for OpenAI {
    async fn gpt_string(&self, prompt: String) -> Result<String, ()> {
        let req = ChatCompletionRequest::new(
            GPT3_5_TURBO_16K.to_string(),
            vec![chat_completion::ChatCompletionMessage {
                role: chat_completion::MessageRole::user,
                content: chat_completion::Content::Text(prompt),
                name: None,
            }],
        );

        let result = self.client.chat_completion(req).unwrap();
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
}

#[derive(Default, Clone)]
pub(crate) struct Ollama {
    client: ollama_rs::Ollama,
}

impl Ollama {
    pub fn new(ip: String, port: u16) -> Self {
        Ollama {
            client: ollama_rs::Ollama::new(ip, port),
        }
    }
}

impl LLM for Ollama {
    async fn gpt_string(&self, prompt: String) -> Result<String, ()> {
        let model = "llama3".to_string();
        let res = self
            .client
            .generate(GenerationRequest::new(model, prompt))
            .await
            .unwrap();
        Ok(res.response.replace(['\"', '\\'], ""))
    }
}
