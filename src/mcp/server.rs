use crate::graph::Node;
use crate::storage::Storage;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::io::{BufRead, Write};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Option<Map<String, Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl std::fmt::Display for JsonRpcResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", serde_json::to_string(self).unwrap_or_default())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResult {
    pub content: Vec<ToolContent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolContent {
    pub r#type: String,
    pub text: String,
}

pub struct McpServer {
    storage: Storage,
}

impl McpServer {
    pub fn new(storage: Storage) -> Self {
        Self { storage }
    }

    pub fn run(&self) -> anyhow::Result<()> {
        let stdin = std::io::stdin();
        let mut stdout = std::io::stdout();
        let mut handle = stdin.lock();

        let mut buffer = String::new();

        loop {
            buffer.clear();
            match handle.read_line(&mut buffer) {
                Ok(0) => break,
                Ok(_) => {
                    let trimmed = buffer.trim();
                    if trimmed.is_empty() {
                        continue;
                    }

                    match serde_json::from_str::<JsonRpcRequest>(trimmed) {
                        Ok(request) => {
                            let response = self.handle_request(&request);
                            if let Some(resp) = response {
                                stdout.write_all(resp.to_string().as_bytes())?;
                                stdout.write_all(b"\n")?;
                                stdout.flush()?;

                                if request.method == "shutdown" {
                                    break;
                                }
                            }
                        }
                        Err(e) => {
                            let error_resp = JsonRpcResponse {
                                jsonrpc: "2.0".to_string(),
                                id: Some(Value::Null),
                                result: None,
                                error: Some(JsonRpcError {
                                    code: -32700,
                                    message: format!("Parse error: {}", e),
                                    data: None,
                                }),
                            };
                            stdout.write_all(error_resp.to_string().as_bytes())?;
                            stdout.write_all(b"\n")?;
                            stdout.flush()?;
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Read error: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }

    fn handle_request(&self, request: &JsonRpcRequest) -> Option<JsonRpcResponse> {
        match request.method.as_str() {
            "initialize" => Some(self.handle_initialize(request)),
            "tools/list" => Some(self.handle_tools_list(request)),
            "tools/call" => Some(self.handle_tools_call(request)),
            "shutdown" => Some(self.handle_shutdown(request)),
            "ping" => Some(self.handle_ping(request)),
            _ => Some(JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id.clone(),
                result: None,
                error: Some(JsonRpcError {
                    code: -32601,
                    message: format!("Method not found: {}", request.method),
                    data: None,
                }),
            }),
        }
    }

    fn handle_initialize(&self, request: &JsonRpcRequest) -> JsonRpcResponse {
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id.clone(),
            result: Some(serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {
                    "tools": {}
                },
                "serverInfo": {
                    "name": "uncverkg",
                    "version": "0.1.0"
                }
            })),
            error: None,
        }
    }

    fn handle_tools_list(&self, request: &JsonRpcRequest) -> JsonRpcResponse {
        let tools = vec![
            Tool {
                name: "search".to_string(),
                description: "Search the knowledge graph for nodes matching a query".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Search query string"
                        },
                        "tier": {
                            "type": "string",
                            "description": "Filter by tier: gold, draft, or unverified",
                            "enum": ["gold", "draft", "unverified"]
                        },
                        "global": {
                            "type": "boolean",
                            "description": "Search global knowledge graph (~/.uncverkg/) instead of project"
                        }
                    },
                    "required": ["query"]
                }),
            },
            Tool {
                name: "write".to_string(),
                description: "Write a new fact to the knowledge graph".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "subject": {
                            "type": "string",
                            "description": "Subject of the fact"
                        },
                        "predicate": {
                            "type": "string",
                            "description": "Predicate/relation (e.g., WORKS_ON, HAS_FEATURE)"
                        },
                        "object": {
                            "type": "string",
                            "description": "Object of the fact"
                        },
                        "global": {
                            "type": "boolean",
                            "description": "Write to global knowledge graph instead of project"
                        }
                    },
                    "required": ["subject", "predicate", "object"]
                }),
            },
            Tool {
                name: "read".to_string(),
                description: "Read a specific node by ID or label".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "query": {
                            "type": "string",
                            "description": "Node ID (UUID) or label to search for"
                        },
                        "global": {
                            "type": "boolean",
                            "description": "Search global knowledge graph instead of project"
                        }
                    },
                    "required": ["query"]
                }),
            },
            Tool {
                name: "stats".to_string(),
                description: "Get knowledge graph statistics".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "global": {
                            "type": "boolean",
                            "description": "Get stats from global knowledge graph instead of project"
                        }
                    }
                }),
            },
            Tool {
                name: "bulk_write".to_string(),
                description: "Bulk write facts from JSON array".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "facts": {
                            "type": "array",
                            "description": "Array of facts to write",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "subject": {"type": "string"},
                                    "predicate": {"type": "string"},
                                    "object": {"type": "string"}
                                },
                                "required": ["subject", "predicate", "object"]
                            }
                        },
                        "global": {
                            "type": "boolean",
                            "description": "Write to global knowledge graph instead of project"
                        }
                    },
                    "required": ["facts"]
                }),
            },
            Tool {
                name: "list_nodes".to_string(),
                description: "List all nodes in the knowledge graph".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "limit": {
                            "type": "number",
                            "description": "Maximum number of nodes to return (default: 50)"
                        },
                        "global": {
                            "type": "boolean",
                            "description": "List from global knowledge graph instead of project"
                        }
                    }
                }),
            },
        ];

        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id.clone(),
            result: Some(serde_json::json!({
                "tools": tools
            })),
            error: None,
        }
    }

    fn handle_tools_call(&self, request: &JsonRpcRequest) -> JsonRpcResponse {
        let params = match &request.params {
            Some(p) => p.clone(),
            None => {
                return JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: request.id.clone(),
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32602,
                        message: "Missing params".to_string(),
                        data: None,
                    }),
                };
            }
        };

        let tool_name = match params.get("name").and_then(|v| v.as_str()) {
            Some(n) => n,
            None => {
                return JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: request.id.clone(),
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32602,
                        message: "Missing tool name".to_string(),
                        data: None,
                    }),
                };
            }
        };

        let arguments: Map<String, Value> = params
            .get("arguments")
            .and_then(|v| v.as_object())
            .cloned()
            .unwrap_or_default();

        let result = match tool_name {
            "search" => self.tool_search(&arguments),
            "write" => self.tool_write(&arguments),
            "read" => self.tool_read(&arguments),
            "stats" => self.tool_stats(&arguments),
            "bulk_write" => self.tool_bulk_write(&arguments),
            "list_nodes" => self.tool_list_nodes(&arguments),
            _ => ToolCallResult {
                content: vec![ToolContent {
                    r#type: "text".to_string(),
                    text: format!("Unknown tool: {}", tool_name),
                }],
                is_error: Some(true),
            },
        };

        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id.clone(),
            result: Some(serde_json::to_value(result).unwrap_or_default()),
            error: None,
        }
    }

    fn handle_shutdown(&self, request: &JsonRpcRequest) -> JsonRpcResponse {
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id.clone(),
            result: Some(serde_json::json!({"success": true})),
            error: None,
        }
    }

    fn handle_ping(&self, request: &JsonRpcRequest) -> JsonRpcResponse {
        JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id.clone(),
            result: Some(serde_json::json!({"pong": true})),
            error: None,
        }
    }

    fn tool_search(&self, args: &Map<String, Value>) -> ToolCallResult {
        let query = match args.get("query").and_then(|v| v.as_str()) {
            Some(q) => q.to_string(),
            None => {
                return ToolCallResult {
                    content: vec![ToolContent {
                        r#type: "text".to_string(),
                        text: "Missing query parameter".to_string(),
                    }],
                    is_error: Some(true),
                };
            }
        };

        let tier = args.get("tier").and_then(|v| v.as_str());
        let global = args
            .get("global")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let graph = if global {
            match Storage::load_global_graph() {
                Some(g) => g,
                None => {
                    return ToolCallResult {
                        content: vec![ToolContent {
                            r#type: "text".to_string(),
                            text: "No global knowledge graph found".to_string(),
                        }],
                        is_error: Some(true),
                    };
                }
            }
        } else {
            self.storage.graph().read().clone()
        };

        let tier_filter = tier.and_then(|t| match t {
            "gold" => Some(crate::graph::Tier::Gold),
            "draft" => Some(crate::graph::Tier::Draft),
            "unverified" => Some(crate::graph::Tier::Unverified),
            _ => None,
        });

        let results: Vec<_> = graph.search(&query, tier_filter);

        let text = if results.is_empty() {
            format!(
                "No nodes found matching '{}' in {}",
                query,
                if global { "~/.uncverkg" } else { "project" }
            )
        } else {
            let graph_name = if global { "~/.uncverkg" } else { "project" };
            let nodes: Vec<String> = results
                .iter()
                .map(|n| {
                    format!(
                        "- {} [{}] (confidence: {})",
                        n.label,
                        format!("{:?}", n.tier),
                        n.confidence.as_u8()
                    )
                })
                .collect();
            format!(
                "Found {} node(s) in {}:\n{}",
                results.len(),
                graph_name,
                nodes.join("\n")
            )
        };

        ToolCallResult {
            content: vec![ToolContent {
                r#type: "text".to_string(),
                text,
            }],
            is_error: None,
        }
    }

    fn tool_write(&self, args: &Map<String, Value>) -> ToolCallResult {
        let subject = match args.get("subject").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => {
                return ToolCallResult {
                    content: vec![ToolContent {
                        r#type: "text".to_string(),
                        text: "Missing subject parameter".to_string(),
                    }],
                    is_error: Some(true),
                };
            }
        };

        let predicate = match args.get("predicate").and_then(|v| v.as_str()) {
            Some(p) => p,
            None => {
                return ToolCallResult {
                    content: vec![ToolContent {
                        r#type: "text".to_string(),
                        text: "Missing predicate parameter".to_string(),
                    }],
                    is_error: Some(true),
                };
            }
        };

        let object = match args.get("object").and_then(|v| v.as_str()) {
            Some(o) => o,
            None => {
                return ToolCallResult {
                    content: vec![ToolContent {
                        r#type: "text".to_string(),
                        text: "Missing object parameter".to_string(),
                    }],
                    is_error: Some(true),
                };
            }
        };

        let global = args
            .get("global")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let storage = if global {
            let global_path = Storage::global_base_path();
            if !global_path.exists() {
                std::fs::create_dir_all(&global_path).ok();
                std::fs::create_dir_all(global_path.join("subgraphs")).ok();
                std::fs::create_dir_all(global_path.join("drafts")).ok();
            }
            match Storage::new(&global_path) {
                Ok(s) => s,
                Err(e) => {
                    return ToolCallResult {
                        content: vec![ToolContent {
                            r#type: "text".to_string(),
                            text: format!("Failed to access global storage: {}", e),
                        }],
                        is_error: Some(true),
                    };
                }
            }
        } else {
            self.storage.clone()
        };

        let graph = storage.graph();
        let mut g = graph.write();

        let subgraph_id = if g.subgraphs.is_empty() {
            g.create_subgraph("main".to_string())
        } else {
            g.subgraphs.keys().next().copied().unwrap()
        };

        let mut node = Node::new(format!("{} --{}--> {}", subject, predicate, object));
        node.properties
            .insert("subject".to_string(), serde_json::json!(subject));
        node.properties
            .insert("predicate".to_string(), serde_json::json!(predicate));
        node.properties
            .insert("object".to_string(), serde_json::json!(object));

        let node_id = g.add_to_subgraph(subgraph_id, node).unwrap_or_else(|| {
            let n = Node::new(format!("{} --{}--> {}", subject, predicate, object));
            g.add_to_drafts(n)
        });

        drop(g);

        if let Some(sg) = storage.graph().read().subgraphs.get(&subgraph_id) {
            let _ = storage.save_subgraph(sg);
        }
        let _ = storage.save_main_network();

        ToolCallResult {
            content: vec![ToolContent {
                r#type: "text".to_string(),
                text: format!(
                    "✅ Wrote: {} --{}--> {}\n   Node ID: {}",
                    subject, predicate, object, node_id
                ),
            }],
            is_error: None,
        }
    }

    fn tool_read(&self, args: &Map<String, Value>) -> ToolCallResult {
        let query = match args.get("query").and_then(|v| v.as_str()) {
            Some(q) => q.to_string(),
            None => {
                return ToolCallResult {
                    content: vec![ToolContent {
                        r#type: "text".to_string(),
                        text: "Missing query parameter".to_string(),
                    }],
                    is_error: Some(true),
                };
            }
        };

        let global = args
            .get("global")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let graph = if global {
            match Storage::load_global_graph() {
                Some(g) => g,
                None => {
                    return ToolCallResult {
                        content: vec![ToolContent {
                            r#type: "text".to_string(),
                            text: "No global knowledge graph found".to_string(),
                        }],
                        is_error: Some(true),
                    };
                }
            }
        } else {
            self.storage.graph().read().clone()
        };

        if let Ok(uuid) = uuid::Uuid::parse_str(&query) {
            if let Some(node) = graph.drafts.get(&uuid) {
                return self.format_node(node);
            }
            for sg in graph.subgraphs.values() {
                if let Some(node) = sg.nodes.get(&uuid) {
                    return self.format_node(node);
                }
            }
        }

        let results = graph.search(&query, None);
        if results.is_empty() {
            ToolCallResult {
                content: vec![ToolContent {
                    r#type: "text".to_string(),
                    text: format!("No node found matching '{}'", query),
                }],
                is_error: Some(true),
            }
        } else if results.len() == 1 {
            self.format_node(&results[0])
        } else {
            let nodes: Vec<String> = results
                .iter()
                .map(|n| format!("- {} (ID: {})", n.label, n.id))
                .collect();
            ToolCallResult {
                content: vec![ToolContent {
                    r#type: "text".to_string(),
                    text: format!("Found {} nodes:\n{}", results.len(), nodes.join("\n")),
                }],
                is_error: None,
            }
        }
    }

    fn tool_stats(&self, args: &Map<String, Value>) -> ToolCallResult {
        let global = args
            .get("global")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let (graph, name) = if global {
            match Storage::load_global_graph() {
                Some(g) => (g, "~/.uncverkg".to_string()),
                None => {
                    return ToolCallResult {
                        content: vec![ToolContent {
                            r#type: "text".to_string(),
                            text: "No global knowledge graph found".to_string(),
                        }],
                        is_error: Some(true),
                    };
                }
            }
        } else {
            (self.storage.graph().read().clone(), "project".to_string())
        };

        let subgraphs = graph.subgraphs.len();
        let nodes: usize = graph.subgraphs.values().map(|sg| sg.node_count()).sum();
        let drafts = graph.drafts.len();

        let mut text = format!(
            "📊 Knowledge Graph Stats ({})\n\
             📁 Subgraphs: {}\n\
             📝 Nodes: {}\n\
             📝 Drafts: {}",
            name, subgraphs, nodes, drafts
        );

        for sg in graph.subgraphs.values() {
            text.push_str(&format!("\n   - {}: {} nodes", sg.name, sg.node_count()));
        }

        ToolCallResult {
            content: vec![ToolContent {
                r#type: "text".to_string(),
                text,
            }],
            is_error: None,
        }
    }

    fn tool_bulk_write(&self, args: &Map<String, Value>) -> ToolCallResult {
        let facts = match args.get("facts").and_then(|v| v.as_array()) {
            Some(f) => f.clone(),
            None => {
                return ToolCallResult {
                    content: vec![ToolContent {
                        r#type: "text".to_string(),
                        text: "Missing facts parameter".to_string(),
                    }],
                    is_error: Some(true),
                };
            }
        };

        let global = args
            .get("global")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let storage = if global {
            let global_path = Storage::global_base_path();
            if !global_path.exists() {
                std::fs::create_dir_all(&global_path).ok();
                std::fs::create_dir_all(global_path.join("subgraphs")).ok();
                std::fs::create_dir_all(global_path.join("drafts")).ok();
            }
            match Storage::new(&global_path) {
                Ok(s) => s,
                Err(e) => {
                    return ToolCallResult {
                        content: vec![ToolContent {
                            r#type: "text".to_string(),
                            text: format!("Failed to access storage: {}", e),
                        }],
                        is_error: Some(true),
                    };
                }
            }
        } else {
            self.storage.clone()
        };

        let graph = storage.graph();
        let mut g = graph.write();

        let subgraph_id = if g.subgraphs.is_empty() {
            g.create_subgraph("main".to_string())
        } else {
            g.subgraphs.keys().next().copied().unwrap()
        };

        let mut written = 0;
        for fact in facts {
            let subject = fact.get("subject").and_then(|v| v.as_str()).unwrap_or("");
            let predicate = fact.get("predicate").and_then(|v| v.as_str()).unwrap_or("");
            let object = fact.get("object").and_then(|v| v.as_str()).unwrap_or("");

            if !subject.is_empty() && !predicate.is_empty() {
                let mut node = Node::new(format!("{} --{}--> {}", subject, predicate, object));
                node.properties
                    .insert("subject".to_string(), serde_json::json!(subject));
                node.properties
                    .insert("predicate".to_string(), serde_json::json!(predicate));
                node.properties
                    .insert("object".to_string(), serde_json::json!(object));

                let _ = g.add_to_subgraph(subgraph_id, node);
                written += 1;
            }
        }

        drop(g);

        if let Some(sg) = storage.graph().read().subgraphs.get(&subgraph_id) {
            let _ = storage.save_subgraph(sg);
        }
        let _ = storage.save_main_network();

        ToolCallResult {
            content: vec![ToolContent {
                r#type: "text".to_string(),
                text: format!(
                    "✅ Wrote {} facts to {}",
                    written,
                    if global { "global" } else { "project" }
                ),
            }],
            is_error: None,
        }
    }

    fn tool_list_nodes(&self, args: &Map<String, Value>) -> ToolCallResult {
        let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(50) as usize;
        let global = args
            .get("global")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let graph = if global {
            match Storage::load_global_graph() {
                Some(g) => g,
                None => {
                    return ToolCallResult {
                        content: vec![ToolContent {
                            r#type: "text".to_string(),
                            text: "No global knowledge graph found".to_string(),
                        }],
                        is_error: Some(true),
                    };
                }
            }
        } else {
            self.storage.graph().read().clone()
        };

        let mut nodes: Vec<_> = graph
            .subgraphs
            .values()
            .flat_map(|sg| sg.nodes.values())
            .collect();
        nodes.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

        let nodes: Vec<String> = nodes
            .into_iter()
            .take(limit)
            .map(|n| format!("- {} [{}] (ID: {})", n.label, format!("{:?}", n.tier), n.id))
            .collect();

        let text = if nodes.is_empty() {
            "No nodes found".to_string()
        } else {
            format!("Nodes:\n{}", nodes.join("\n"))
        };

        ToolCallResult {
            content: vec![ToolContent {
                r#type: "text".to_string(),
                text,
            }],
            is_error: None,
        }
    }

    fn format_node(&self, node: &Node) -> ToolCallResult {
        let mut text = format!(
            "📝 Node: {}\n   ID: {}\n   Tier: {:?}\n   Confidence: {}",
            node.label,
            node.id,
            node.tier,
            node.confidence.as_u8()
        );

        if !node.properties.is_empty() {
            text.push_str("\n   Properties:");
            for (k, v) in &node.properties {
                text.push_str(&format!("\n     {}: {}", k, v));
            }
        }

        ToolCallResult {
            content: vec![ToolContent {
                r#type: "text".to_string(),
                text,
            }],
            is_error: None,
        }
    }
}
