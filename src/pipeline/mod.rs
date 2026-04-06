use crate::providers::{LLMProvider, Message, OllamaProvider, ToolCall as ProviderToolCall};
use crate::tools::{ToolCall, ToolRegistry};
use crate::storage::Storage;
use std::sync::Arc;
use std::sync::Mutex;
use tracing::{info, debug, error};

#[derive(Clone)]
pub struct AgentPipeline {
    provider: OllamaProvider,
    tool_registry: Arc<ToolRegistry>,
    storage: Storage,
    model: String,
    tools: Vec<crate::tools::Tool>,
}

impl AgentPipeline {
    pub fn new(provider: OllamaProvider, tool_registry: Arc<ToolRegistry>, storage: Storage, model: String) -> Self {
        let tools = tool_registry.tools().to_vec();
        info!("AgentPipeline created with {} tools", tools.len());
        Self {
            provider,
            tool_registry,
            storage,
            model,
            tools,
        }
    }

    pub async fn query(&self, user_input: &str) -> String {
        info!("[QUERY] Starting query: {}", user_input);
        
        let context = self.research(user_input).await;
        debug!("[QUERY] Research context: {}", context);
        
        let response = self.actor(user_input, &context).await;
        
        info!("[QUERY] Complete, response length: {}", response.len());
        response
    }

    pub async fn query_with_learn(&self, user_input: &str, _response: &str) -> bool {
        let context = self.research(user_input).await;
        let resp = self.actor(user_input, &context).await;
        
        let registry = Arc::clone(&self.tool_registry);
        let storage = self.storage.clone();
        let graph = registry.graph().clone();
        let tools = self.tools.clone();
        
        if let Ok(res) = Self::run_reviewer(&self.model, user_input, &resp, &tools).await {
            for tc in res.tool_calls.iter().flat_map(|tcs| tcs.iter()) {
                registry.execute(ToolCall {
                    tool_name: tc.name.clone(),
                    arguments: tc.arguments.clone(),
                });
            }
            
            // Save drafts
            for (_, draft) in graph.read().drafts.iter() {
                let _ = storage.save_draft(draft);
            }
            
            // Save subgraphs
            for (_, subgraph) in graph.read().subgraphs.iter() {
                let _ = storage.save_subgraph(subgraph);
            }
            
            // Save main_network
            let _ = storage.save_main_network();
            
            return true;
        }
        false
    }

    pub fn learn(&self, user_input: &str, response: &str) {
        info!("Starting background learning for: {}", user_input);
        let registry = Arc::clone(&self.tool_registry);
        let storage = self.storage.clone();
        let graph = registry.graph().clone();
        let model = self.model.clone();
        let tools = self.tools.clone();
        let input = user_input.to_string();
        let resp = response.to_string();

        tokio::spawn(async move {
            info!("[LEARN] Background: Starting reviewer for async learning");
            match Self::run_reviewer(&model, &input, &resp, &tools).await {
                Ok(res) => {
                    let count = res.tool_calls.as_ref().map(|t| t.len()).unwrap_or(0);
                    info!("[LEARN] Background: Reviewer completed, {} tool calls", count);
                    for tc in res.tool_calls.iter().flat_map(|tcs| tcs.iter()) {
                        debug!("[LEARN] Background: Executing tool {}: {:?}", tc.name, tc.arguments);
                        registry.execute(ToolCall {
                            tool_name: tc.name.clone(),
                            arguments: tc.arguments.clone(),
                        });
                    }
                    
                    // Save drafts
                    let drafts_count = graph.read().drafts.len();
                    for (_, draft) in graph.read().drafts.iter() {
                        if let Err(e) = storage.save_draft(draft) {
                            error!("[LEARN] Background: Failed to save draft: {}", e);
                        }
                    }
                    
                    // Save any subgraphs that have been modified
                    let subgraphs_count = graph.read().subgraphs.len();
                    for (_, subgraph) in graph.read().subgraphs.iter() {
                        if let Err(e) = storage.save_subgraph(subgraph) {
                            error!("[LEARN] Background: Failed to save subgraph: {}", e);
                        }
                    }
                    
                    info!("[LEARN] Background: Learning complete, saved {} drafts, {} subgraphs", drafts_count, subgraphs_count);
                }
                Err(e) => {
                    error!("[LEARN] Background: Reviewer failed: {}", e);
                }
            }
        });
    }

    pub async fn research(&self, query: &str) -> String {
        let system = format!(
            r#"You are the Researcher. Search the knowledge graph for what YOU have learned about this topic.

Query: "{}"

The knowledge graph contains what the AI (you) has learned from previous conversations - NOT information about the user.
Search for relevant facts, concepts, or relationships you have learned about.
Return a brief summary of what you found."#,
            query
        );

        let request = crate::providers::LLMRequest {
            model: self.model.clone(),
            messages: vec![
                Message { role: "system".to_string(), content: system },
                Message { role: "user".to_string(), content: format!("Search for: {}", query) },
            ],
            temperature: 0.3,
            max_tokens: Some(512),
            tools: Some(self.tools.clone()),
        };

        let result = self.provider.complete(request).await;
        self.handle_tool_calls(result)
    }

    pub async fn actor(&self, query: &str, context: &str) -> String {
        let context_str = if context.is_empty() {
            "No prior knowledge found".to_string()
        } else {
            context.to_string()
        };

        let system = format!(
            r#"You are a helpful AI assistant. Answer the user's question using the provided context.

User question: "{}"

Context from knowledge graph:
{}

Your response should:
- Be comprehensive and informative
- Use the context to provide specific details
- If context is empty, answer from your general knowledge"#,
            query, context_str
        );

        let request = crate::providers::LLMRequest {
            model: self.model.clone(),
            messages: vec![
                Message { role: "system".to_string(), content: system },
            ],
            temperature: 0.7,
            max_tokens: Some(1024),
            tools: None,
        };

        self.provider
            .complete(request)
            .await
            .map(|r| r.content)
            .unwrap_or_else(|_| "I couldn't generate a response.".to_string())
    }

    pub async fn query_streaming<F>(&self, user_input: &str, on_chunk: F) -> String
    where
        F: FnMut(String, Option<Vec<ProviderToolCall>>) + Send + 'static,
    {
        info!("[QUERY] Starting streaming query: {}", user_input);
        
        // Research phase (non-streaming for now, simpler)
        let context = self.research(user_input).await;
        debug!("[QUERY] Research context: {}", context);
        
        // Actor phase - streaming
        let context_str = if context.is_empty() {
            "No prior knowledge found".to_string()
        } else {
            context.to_string()
        };

        let system = format!(
            r#"You are a helpful AI assistant. Answer the user's question using the provided context.

User question: "{}"

Context from knowledge graph:
{}

Your response should:
- Be comprehensive and informative
- Use the context to provide specific details
- If context is empty, answer from your general knowledge"#,
            user_input, context_str
        );

        let request = crate::providers::LLMRequest {
            model: self.model.clone(),
            messages: vec![
                Message { role: "system".to_string(), content: system },
            ],
            temperature: 0.7,
            max_tokens: Some(2048),
            tools: Some(self.tools.clone()),
        };

        // Use Arc + Mutex to share callback safely
        let on_chunk = Arc::new(Mutex::new(on_chunk));
        let on_chunk_clone = Arc::clone(&on_chunk);

        let request_clone = request.clone();
        
        // Start streaming in background
        let provider = self.provider.clone();
        let handle = tokio::spawn(async move {
            provider.complete_streaming(request_clone, move |content, tool_calls| {
                if let Ok(mut cb) = on_chunk_clone.lock() {
                    cb(content, tool_calls);
                }
            }).await
        });

        let response = handle.await.unwrap_or(Err(crate::providers::ProviderError::Other("Task failed".to_string())));
        
        match response {
            Ok(resp) => {
                // Handle any tool calls from the response
                if let Some(tcs) = resp.tool_calls {
                    for tc in tcs {
                        let tool_call = ToolCall {
                            tool_name: tc.name,
                            arguments: tc.arguments,
                        };
                        let result = self.tool_registry.execute(tool_call);
                        if result.success {
                            debug!("[QUERY] Tool executed successfully");
                        }
                    }
                }
                info!("[QUERY] Streaming complete, response length: {}", resp.content.len());
                resp.content
            }
            Err(e) => {
                error!("[QUERY] Streaming error: {}", e);
                format!("Error: {}", e)
            }
        }
    }

    async fn run_reviewer(
        model: &str,
        input: &str,
        response: &str,
        tools: &[crate::tools::Tool],
    ) -> Result<crate::providers::LLMResponse, crate::providers::ProviderError> {
        let system = format!(
            r#"You are the Reviewer. Extract what YOU (the AI) have learned from this conversation.

The knowledge graph stores what the AI should remember - including both:
1. Topics/concepts discussed (what you learned)
2. Information the user shared about themselves

User said: "{}"
You responded: "{}"

Extract factual information as JSON array:
[{{"subject": "topic/concept OR User", "predicate": "relation", "object": "detail"}}]

IMPORTANT: For related concepts, also extract edges:
[{{"subject": "concept_a", "predicate": "RELATES_TO", "object": "concept_b"}}]

Examples:
- ✅ "User works with Rust" -> {{"subject": "User", "predicate": "WORKS_ON", "object": "Rust"}}
- ✅ "Rust has memory management" -> {{"subject": "Rust", "predicate": "HAS_FEATURE", "object": "memory management"}}
- ✅ Related: {{"subject": "kg-core", "predicate": "USES", "object": "Rust"}}

If no new facts, return: []

Use upsert_node tool to add nodes and create_edge tool to connect related nodes with confidence: 70"#,
            input, response
        );

        let request = crate::providers::LLMRequest {
            model: model.to_string(),
            messages: vec![
                Message { role: "system".to_string(), content: system },
            ],
            temperature: 0.3,
            max_tokens: Some(512),
            tools: Some(tools.to_vec()),
        };

        Ok(crate::providers::OllamaProvider::default_model("gemma2:2b").complete(request).await?)
    }

    fn handle_tool_calls(
        &self,
        result: Result<crate::providers::LLMResponse, crate::providers::ProviderError>,
    ) -> String {
        if let Ok(res) = &result {
            if let Some(tcs) = &res.tool_calls {
                for tc in tcs {
                    self.tool_registry.execute(ToolCall {
                        tool_name: tc.name.clone(),
                        arguments: tc.arguments.clone(),
                    });
                }
            }
            res.content.clone()
        } else {
            String::new()
        }
    }
}
