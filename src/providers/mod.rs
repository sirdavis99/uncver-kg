use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[allow(unused_imports)]
use async_trait::async_trait;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub temperature: f32,
    pub max_tokens: Option<u32>,
    pub tools: Option<Vec<crate::tools::Tool>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LLMResponse {
    pub content: String,
    pub tool_calls: Option<Vec<ToolCall>>,
    pub finish_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub name: String,
    pub arguments: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaTool {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

impl From<&crate::tools::Tool> for OllamaTool {
    fn from(t: &crate::tools::Tool) -> Self {
        let mut required = Vec::new();
        let mut properties = serde_json::Map::new();
        
        for param in &t.parameters {
            properties.insert(param.name.clone(), serde_json::json!({
                "type": param.param_type,
                "description": param.description
            }));
            if param.required {
                required.push(param.name.clone());
            }
        }
        
        OllamaTool {
            name: t.name.clone(),
            description: t.description.clone(),
            parameters: serde_json::json!({
                "type": "object",
                "properties": properties,
                "required": required
            }),
        }
    }
}

#[allow(async_fn_in_trait)]
pub trait LLMProvider: Send + Sync {
    fn name(&self) -> &str;
    fn model(&self) -> &str;
    
    async fn complete(&self, request: LLMRequest) -> Result<LLMResponse, ProviderError>;
    
    async fn complete_streaming<F>(&self, request: LLMRequest, on_chunk: F) -> Result<LLMResponse, ProviderError>
    where
        F: Fn(String, Option<Vec<ToolCall>>) + Send + Sync + 'static;
    
    async fn extract_deductions(
        &self,
        conversation: &[Message],
        existing_facts: &[String],
    ) -> Result<Vec<Deduction>, ProviderError>;
    
    async fn calculate_confidence(
        &self,
        fact: &str,
        conversation: &[Message],
    ) -> Result<u8, ProviderError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deduction {
    pub subject: String,
    pub predicate: String,
    pub object: String,
}

#[derive(Debug, Clone)]
pub enum ProviderError {
    Network(String),
    Parse(String),
    RateLimit,
    ModelNotFound,
    Unauthorized,
    Other(String),
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderError::Network(s) => write!(f, "Network error: {}", s),
            ProviderError::Parse(s) => write!(f, "Parse error: {}", s),
            ProviderError::RateLimit => write!(f, "Rate limit exceeded"),
            ProviderError::ModelNotFound => write!(f, "Model not found"),
            ProviderError::Unauthorized => write!(f, "Unauthorized"),
            ProviderError::Other(s) => write!(f, "Error: {}", s),
        }
    }
}

impl std::error::Error for ProviderError {}

#[derive(Clone)]
pub struct OllamaProvider {
    base_url: String,
    model: String,
    client: reqwest::Client,
}

impl OllamaProvider {
    pub fn new(base_url: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            model: model.into(),
            client: reqwest::Client::new(),
        }
    }

    pub fn default_model(model: impl Into<String>) -> Self {
        Self::new("http://localhost:11434", model)
    }

    fn parse_streaming_response_with_tools(&self, body: &str) -> Result<(String, Option<Vec<ToolCall>>), ProviderError> {
        let mut final_content = String::new();
        let mut tool_calls = None;
        
        for line in body.lines() {
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(chunk) = serde_json::from_str::<serde_json::Value>(line) {
                if let Some(content) = chunk["message"]["content"].as_str() {
                    if !content.is_empty() {
                        final_content.push_str(content);
                    }
                }
                
                if let Some(tc_array) = chunk["message"]["tool_calls"].as_array() {
                    let calls: Vec<ToolCall> = tc_array.iter()
                        .filter_map(|tc| {
                            let name = tc["function"]["name"].as_str()?.to_string();
                            
                            let args: HashMap<String, serde_json::Value> = if let Some(args_obj) = tc["function"]["arguments"].as_object() {
                                args_obj.iter()
                                    .map(|(k, v)| (k.clone(), v.clone()))
                                    .collect()
                            } else if let Some(args_str) = tc["function"]["arguments"].as_str() {
                                serde_json::from_str(args_str).ok().unwrap_or_default()
                            } else {
                                HashMap::new()
                            };
                            
                            Some(ToolCall { name, arguments: args })
                        })
                        .collect();
                    if !calls.is_empty() {
                        tool_calls = Some(calls);
                    }
                }
            }
        }
        
        Ok((final_content, tool_calls))
    }
}

impl LLMProvider for OllamaProvider {
    fn name(&self) -> &str {
        "ollama"
    }

    fn model(&self) -> &str {
        &self.model
    }

    async fn complete(&self, request: LLMRequest) -> Result<LLMResponse, ProviderError> {
        let url = format!("{}/api/chat", self.base_url);
        
        let tools: Option<Vec<serde_json::Value>> = request.tools.as_ref().map(|t| {
            t.iter().map(|tool| {
                let ollama_tool: OllamaTool = tool.into();
                serde_json::json!({
                    "type": "function",
                    "function": ollama_tool
                })
            }).collect()
        });
        
        let ollama_request: serde_json::Value = serde_json::json!({
            "model": request.model,
            "messages": request.messages,
            "temperature": request.temperature,
            "max_tokens": request.max_tokens.unwrap_or(4096),
            "tools": tools,
            "think": false
        });

        let response = self.client
            .post(&url)
            .json(&ollama_request)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;

        if !response.status().is_success() {
            return Err(ProviderError::Other(format!(
                "HTTP error: {}",
                response.status()
            )));
        }
        
        let body = response.text().await.map_err(|e| ProviderError::Parse(e.to_string()))?;
        
        let (content, tool_calls) = self.parse_streaming_response_with_tools(&body)?;

        Ok(LLMResponse {
            content,
            tool_calls,
            finish_reason: "stop".to_string(),
        })
    }

    async fn complete_streaming<F>(&self, request: LLMRequest, on_chunk: F) -> Result<LLMResponse, ProviderError>
    where
        F: Fn(String, Option<Vec<ToolCall>>) + Send + Sync + 'static,
    {
        let url = format!("{}/api/chat", self.base_url);
        
        let tools: Option<Vec<serde_json::Value>> = request.tools.as_ref().map(|t| {
            t.iter().map(|tool| {
                let ollama_tool: OllamaTool = tool.into();
                serde_json::json!({
                    "type": "function",
                    "function": ollama_tool
                })
            }).collect()
        });
        
        let ollama_request: serde_json::Value = serde_json::json!({
            "model": request.model,
            "messages": request.messages,
            "temperature": request.temperature,
            "max_tokens": request.max_tokens.unwrap_or(4096),
            "tools": tools,
            "stream": true
        });
        
        let mut final_content = String::new();
        let mut tool_calls: Option<Vec<ToolCall>> = None;
        
        let response = self.client
            .post(&url)
            .json(&ollama_request)
            .send()
            .await
            .map_err(|e| ProviderError::Network(e.to_string()))?;
        
        if !response.status().is_success() {
            return Err(ProviderError::Other(format!(
                "HTTP error: {}",
                response.status()
            )));
        }
        
        let mut stream = response.bytes_stream();
        let mut buffer = String::new();
        
        use futures::stream::StreamExt;
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|e| ProviderError::Network(e.to_string()))?;
            if let Ok(text) = String::from_utf8(chunk.to_vec()) {
                buffer.push_str(&text);
                
                while let Some(newline_idx) = buffer.find('\n') {
                    let line = buffer[..newline_idx].to_string();
                    buffer = buffer[newline_idx + 1..].to_string();
                    
                    if line.trim().is_empty() {
                        continue;
                    }
                    
                if let Ok(chunk_json) = serde_json::from_str::<serde_json::Value>(&line) {
                    if let Some(content) = chunk_json["message"]["content"].as_str() {
                        if !content.is_empty() {
                            final_content.push_str(content);
                            // Only pass the NEW content, not the accumulated
                            on_chunk(content.to_string(), None);
                        }
                    }

                    if let Some(tc_array) = chunk_json["message"]["tool_calls"].as_array() {
                        let calls: Vec<ToolCall> = tc_array.iter()
                            .filter_map(|tc| {
                                let name = tc["function"]["name"].as_str()?.to_string();

                                let args: HashMap<String, serde_json::Value> = if let Some(args_obj) = tc["function"]["arguments"].as_object() {
                                    args_obj.iter()
                                        .map(|(k, v)| (k.clone(), v.clone()))
                                        .collect()
                                } else if let Some(args_str) = tc["function"]["arguments"].as_str() {
                                    serde_json::from_str(args_str).ok().unwrap_or_default()
                                } else {
                                    HashMap::new()
                                };

                                Some(ToolCall { name, arguments: args })
                            })
                            .collect();
                        if !calls.is_empty() {
                            tool_calls = Some(calls);
                        }
                    }
                }
                }
            }
        }
        
        Ok(LLMResponse {
            content: final_content,
            tool_calls,
            finish_reason: "stop".to_string(),
        })
    }

    async fn extract_deductions(
        &self,
        conversation: &[Message],
        existing_facts: &[String],
    ) -> Result<Vec<Deduction>, ProviderError> {
        let system_prompt = format!(
            r#"You are a knowledge extraction system. Analyze the conversation and extract structured facts (subject-predicate-object triples).

Rules:
- Only extract permanent, factual information
- Ignore conversational filler like "hello", "thanks", etc.
- Output ONLY valid JSON array

Example output:
[{{"subject": "David", "predicate": "WORKS_ON", "object": "Rust project"}}, {{"subject": "Project", "predicate": "HAS_NAME", "object": "kg-core"}}]

Existing facts to avoid duplicates:
{}

Conversation:
{}"#,
            existing_facts.join("\n"),
            conversation.iter()
                .map(|m| format!("{}: {}", m.role, m.content))
                .collect::<Vec<_>>()
                .join("\n")
        );

        let request = LLMRequest {
            model: self.model.clone(),
            messages: vec![Message {
                role: "system".to_string(),
                content: system_prompt,
            }],
            temperature: 0.3,
            max_tokens: Some(1024),
            tools: None,
        };

        let response = self.complete(request).await?;
        
        let content = response.content.trim();
        
        let json_start = content.find('[').unwrap_or(content.len());
        let json_end = content.rfind(']').map_or(content.len(), |i| i + 1);
        let json_str = &content[json_start..json_end];
        
        if json_str.is_empty() {
            return Ok(Vec::new());
        }
        
        let deductions: Vec<Deduction> = serde_json::from_str(json_str)
            .or_else(|_| {
                serde_json::from_str(&format!("[{}]", json_str))
            })
            .unwrap_or_default();

        Ok(deductions)
    }

    async fn calculate_confidence(
        &self,
        fact: &str,
        conversation: &[Message],
    ) -> Result<u8, ProviderError> {
        let system_prompt = format!(
            r#"Analyze the confidence level of the following fact based on the conversation.

Fact: {}

Conversation:
{}

Respond with ONLY a number from 0-100 representing the confidence score.

Scoring guidelines:
- Explicit user confirmation (+40): "Yes, that's right", "Correct"
- Repetition (+30): Same fact mentioned 3+ times
- Logical consistency (+20): Doesn't contradict existing facts
- Certainty language (+10): "is" vs "might be", "definitely" vs "probably"#,
            fact,
            conversation.iter()
                .map(|m| format!("{}: {}", m.role, m.content))
                .collect::<Vec<_>>()
                .join("\n")
        );

        let request = LLMRequest {
            model: self.model.clone(),
            messages: vec![Message {
                role: "system".to_string(),
                content: system_prompt,
            }],
            temperature: 0.1,
            max_tokens: Some(10),
            tools: None,
        };

        let response = self.complete(request).await?;
        
        let score = response.content
            .trim()
            .parse::<u8>()
            .unwrap_or(50)
            .min(100);

        Ok(score)
    }
}

pub struct ProgrammaticProvider;

impl ProgrammaticProvider {
    pub fn new() -> Self {
        Self
    }

    pub fn extract_facts_simple(&self, text: &str) -> Vec<Deduction> {
        let mut deductions = Vec::new();
        
        let patterns = [
            (r"(\w+)\s+is\s+(.+?)(?:\.|$)", "IS_A"),
            (r"(\w+)\s+works\s+(?:at|on)\s+(.+?)(?:\.|$)", "WORKS_ON"),
            (r"(\w+)\s+uses\s+(.+?)(?:\.|$)", "USES"),
            (r"(\w+)\s+is\s+(?:a|an)\s+(.+?)(?:\.|$)", "IS_A"),
            (r"my\s+name\s+is\s+(\w+)", "HAS_NAME"),
        ];

        for (pattern, predicate) in patterns {
            if let Ok(re) = regex::Regex::new(pattern) {
                for cap in re.captures_iter(text) {
                    if let (Some(subject), Some(object)) = (cap.get(1), cap.get(2)) {
                        deductions.push(Deduction {
                            subject: subject.as_str().to_string(),
                            predicate: predicate.to_string(),
                            object: object.as_str().trim().to_string(),
                        });
                    }
                }
            }
        }

        deductions
    }

    pub fn calculate_confidence_simple(&self, fact: &str, conversation: &[Message]) -> u8 {
        let mut score = 30u8;

        let fact_lower = fact.to_lowercase();
        
        let confirmations = ["yes", "correct", "right", "exactly", "that's right"];
        for msg in conversation {
            if confirmations.iter().any(|c| msg.content.to_lowercase().contains(c)) {
                score += 40;
                break;
            }
        }

        let mention_count = conversation
            .iter()
            .filter(|m| m.content.to_lowercase().contains(&fact_lower))
            .count();
        
        if mention_count >= 3 {
            score += 30;
        } else if mention_count >= 1 {
            score += 10;
        }

        if fact_lower.contains("might") || fact_lower.contains("probably") || fact_lower.contains("maybe") {
            score = score.saturating_sub(20);
        }

        score.min(100)
    }
}

impl Default for ProgrammaticProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl LLMProvider for ProgrammaticProvider {
    fn name(&self) -> &str {
        "programmatic"
    }

    fn model(&self) -> &str {
        "regex-based"
    }

    async fn complete(&self, _request: LLMRequest) -> Result<LLMResponse, ProviderError> {
        Err(ProviderError::Other("Programmatic provider does not support completion".to_string()))
    }

    async fn complete_streaming<F>(&self, _request: LLMRequest, _on_chunk: F) -> Result<LLMResponse, ProviderError>
    where
        F: Fn(String, Option<Vec<ToolCall>>) + Send + Sync + 'static,
    {
        Err(ProviderError::Other("Programmatic provider does not support streaming".to_string()))
    }

    async fn extract_deductions(
        &self,
        _conversation: &[Message],
        _existing_facts: &[String],
    ) -> Result<Vec<Deduction>, ProviderError> {
        Ok(Vec::new())
    }

    async fn calculate_confidence(
        &self,
        fact: &str,
        conversation: &[Message],
    ) -> Result<u8, ProviderError> {
        Ok(self.calculate_confidence_simple(fact, conversation))
    }
}
