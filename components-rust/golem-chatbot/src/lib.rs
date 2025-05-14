mod bindings;

use crate::bindings::exports::golem::chatbot_exports::chatbot::*;
use crate::bindings::golem::llm::llm::{
    self, ChatEvent, CompleteResponse, ContentPart, ToolCall, ToolDefinition, ToolResult,
};
use chrono::{DateTime, Utc};
use reqwest::{Client, Response};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use std::collections::HashMap;
use std::str::FromStr;
use std::string::ToString;
use std::sync::{LazyLock, RwLock};
use url::Url;

struct State {
    history: Vec<Exchange>,
    context: Vec<String>,
    prompt_urls: Vec<Url>,
}

static STATE: LazyLock<RwLock<State>> = LazyLock::new(|| {
    RwLock::new(State {
        history: vec![],
        context: vec![],
        prompt_urls: vec![],
    })
});

const LLM_RESPONSE_FORMAT: &str = r#"
{
    "response": string, // the full response to the user prompt
    "prompt_urls": array<string> // all the urls discovered from the user prompt
    "documentation_references": array<string> // all the documentation URL references that might help the user finding an answer
}
"#;

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone, Eq, PartialEq)]
struct LlmResponse {
    response: String,
    prompt_urls: Vec<Url>,
    documentation_references: Vec<Url>,
}

struct Component;

impl Guest for Component {
    fn add_context(context: String) {
        STATE.write().unwrap().context.push(context);
    }

    fn get_contexts() -> Vec<String> {
        STATE.read().unwrap().context.clone()
    }

    fn get_history() -> Vec<Exchange> {
        STATE.read().unwrap().history.clone()
    }

    fn prompt(input: String) -> String {
        match process_message(input) {
            Ok(exchange) => exchange.response,
            Err(e) => {
                format!("Error: {}", e)
            }
        }
    }
}

fn process_message(prompt: String) -> Result<Exchange, String> {
    let context_urls = STATE
        .read()
        .unwrap()
        .prompt_urls
        .iter()
        .map(|c| format!("- {}", c))
        .collect::<Vec<String>>()
        .join("\n");
    let context = STATE
        .read()
        .unwrap()
        .context
        .iter()
        .map(|c| format!("- {}", c))
        .collect::<Vec<String>>()
        .join("\n");

    let instructions = format!(
        r#"
                                You are used as part of an application as an assistant that helps developers build and run applications on Golem or Golem Cloud (“Golem Assistant”).
                                
                                You need to primarily search the answer in the official Golem Cloud documentation website.
                                The answer might be in any part of this website and the user might be using any of the languages supported by Golem and Golem Cloud.

                                Make sure to provide links to the documentation that effectively exists. Verify that their content can be really helpful for the user before including it.
                                
                                DOCUMENTATION URL MAIN PAGE: https://learn.golem.cloud
                                
                                ---
                                RESPONSE STRUCTURE:

                                Your responses should have the following json structure:
                                ```
                                {LLM_RESPONSE_FORMAT}
                                ```
                                Your entire response should be a compact and valid json, without spaces or newlines. Do not include any headers / trailers like ``` that would prevent
                                the response from being parsed as json.
                                
                                ---
    
                                CONTEXT:
                                Those elements include all the previous context proided explicitly by the user:
                                ```
                                {}
                                ```
    
                                In the previous responses the following list or URLs was used to expand the context.
                                It's mandatory that you keep into account the content of all those URLs in your response, so you should visit them and consider their content in the response.
                                Only if the list of URLs is empty you're allowed to not consider them.
                                
                                CONTEXT URLS:
                                ```
                                {}
                                ```
                                
                                ---
                                
                                If in the user prompt there are new URLs, you should also visit them and use them to expand the context before responding.
                                You should also add the newly discovered URLs in the user prompt to the list of URLs in the response.
                                Don't add the same URL twice and don't include the ones that are already in the context URLs above.
                                
                                Please provide an answer to the user question with a little bit of reasoning and motivate your answers.
                                If it might help, add any useful link from the documentation website in the response.
                                If possible, your answer should be relevant to the programming language that the user is using.
                            "#,
        &context, &context_urls
    );

    println!("INSTRUCTIONS:\n{}", instructions);
    println!("INPUT:\n{}", prompt);

    let llm_response = ask_model(instructions, prompt.clone());

    println!("{:#?}", llm_response);

    match llm_response {
        Ok(llm_response) => {
            let response = Exchange {
                prompt,
                response: format!(
                    "{}\n Documentation references: {}",
                    llm_response.response,
                    llm_response
                        .documentation_references
                        .iter()
                        .map(|url| format!("{}", url.to_string()))
                        .collect::<Vec<String>>()
                        .join(", ")
                ),
            };
            STATE.write().unwrap().history.push(response.clone());
            Ok(response)
        }
        Err(err) => Err(err),
    }
}

fn ask_model(instructions: String, input: String) -> Result<LlmResponse, String> {
    let open_api_key = std::env::var("OPENAI_API_KEY").expect("OPENAI_API_KEY not present");
    let bearer_token = format!("Bearer {}", open_api_key);
    let client = Client::new();
    let body = serde_json::json!({
        "model": "gpt-4o",
        "temperature": 0.2,
        "stream": false,
        "messages": [
            {
                "role": "system",
                "content": instructions
            },
            {
                "role": "user",
                "content": input
            }
        ],
    });

    let response: Response = client
        .post(&format!("https://api.openai.com/v1/chat/completions"))
        .json(&body)
        .header("Authorization", bearer_token)
        .send()
        .expect("Request failed");

    println!("RAW RESPONSE: {:#?}", response);

    let result: HashMap<String, serde_json::Value> =
        response.json().map_err(|err| err.to_string())?;

    let llm_response = result
        .get("choices")
        .and_then(|choices| choices.get(0))
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(|content| content.as_str())
        .ok_or("No content found in response")?
        .to_string();

    parse_openai_response(llm_response)
}

fn parse_openai_response(json: String) -> Result<LlmResponse, String> {
    let s = json
        .strip_prefix("```json")
        .and_then(|s| s.strip_suffix("```"))
        .unwrap_or(&json)
        .trim();
    serde_json::from_str(s).map_err(|e| e.to_string())
}

bindings::export!(Component with_types_in bindings);
