use crate::graph::{Edge, Graph, Node, Tier};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tool {
    pub name: String,
    pub description: String,
    pub parameters: Vec<ToolParameter>,
    pub can_write: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolParameter {
    pub name: String,
    pub description: String,
    pub param_type: String,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub tool_name: String,
    pub arguments: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub success: bool,
    pub output: serde_json::Value,
    pub error: Option<String>,
}

pub struct ToolRegistry {
    tools: Vec<Tool>,
    graph: Arc<parking_lot::RwLock<Graph>>,
    write_enabled: bool,
}

impl ToolRegistry {
    pub fn new(graph: Arc<parking_lot::RwLock<Graph>>, write_enabled: bool) -> Self {
        let mut tools = vec![
            // Graph tools
            Tool {
                name: "query_graph".to_string(),
                description: "Search for nodes in the knowledge graph by label or topic"
                    .to_string(),
                parameters: vec![
                    ToolParameter {
                        name: "query".to_string(),
                        description: "The search query (label or topic)".to_string(),
                        param_type: "string".to_string(),
                        required: true,
                    },
                    ToolParameter {
                        name: "tier".to_string(),
                        description: "Filter by tier (gold, draft, unverified)".to_string(),
                        param_type: "string".to_string(),
                        required: false,
                    },
                ],
                can_write: false,
            },
            Tool {
                name: "list_all_nodes".to_string(),
                description: "List all nodes in the knowledge graph".to_string(),
                parameters: vec![],
                can_write: false,
            },
            Tool {
                name: "get_node_details".to_string(),
                description: "Get detailed information about a specific node".to_string(),
                parameters: vec![ToolParameter {
                    name: "node_id".to_string(),
                    description: "The UUID of the node".to_string(),
                    param_type: "string".to_string(),
                    required: true,
                }],
                can_write: false,
            },
            // File system tools
            Tool {
                name: "list_directory".to_string(),
                description: "List files and directories in a path".to_string(),
                parameters: vec![ToolParameter {
                    name: "path".to_string(),
                    description: "Directory path to list (default: current directory)".to_string(),
                    param_type: "string".to_string(),
                    required: false,
                }],
                can_write: false,
            },
            Tool {
                name: "read_file".to_string(),
                description: "Read the content of a file".to_string(),
                parameters: vec![ToolParameter {
                    name: "path".to_string(),
                    description: "Path to the file".to_string(),
                    param_type: "string".to_string(),
                    required: true,
                }],
                can_write: false,
            },
            Tool {
                name: "search_code".to_string(),
                description: "Search for text in files (like grep)".to_string(),
                parameters: vec![
                    ToolParameter {
                        name: "pattern".to_string(),
                        description: "Text pattern to search for".to_string(),
                        param_type: "string".to_string(),
                        required: true,
                    },
                    ToolParameter {
                        name: "path".to_string(),
                        description: "Directory to search in".to_string(),
                        param_type: "string".to_string(),
                        required: false,
                    },
                ],
                can_write: false,
            },
        ];

        if write_enabled {
            tools.push(Tool {
                name: "upsert_node".to_string(),
                description: "Create a new node or update an existing one".to_string(),
                parameters: vec![
                    ToolParameter {
                        name: "label".to_string(),
                        description: "The node label".to_string(),
                        param_type: "string".to_string(),
                        required: true,
                    },
                    ToolParameter {
                        name: "properties".to_string(),
                        description: "JSON object of properties".to_string(),
                        param_type: "object".to_string(),
                        required: false,
                    },
                    ToolParameter {
                        name: "confidence".to_string(),
                        description: "Confidence score (0-100)".to_string(),
                        param_type: "number".to_string(),
                        required: false,
                    },
                ],
                can_write: true,
            });

            tools.push(Tool {
                name: "delete_node".to_string(),
                description: "Delete a node from the graph".to_string(),
                parameters: vec![ToolParameter {
                    name: "node_id".to_string(),
                    description: "The UUID of the node to delete".to_string(),
                    param_type: "string".to_string(),
                    required: true,
                }],
                can_write: true,
            });

            tools.push(Tool {
                name: "adjust_weight".to_string(),
                description: "Adjust the confidence score of an existing node".to_string(),
                parameters: vec![
                    ToolParameter {
                        name: "node_id".to_string(),
                        description: "The UUID of the node".to_string(),
                        param_type: "string".to_string(),
                        required: true,
                    },
                    ToolParameter {
                        name: "confidence".to_string(),
                        description: "New confidence score (0-100)".to_string(),
                        param_type: "number".to_string(),
                        required: true,
                    },
                ],
                can_write: true,
            });

            tools.push(Tool {
                name: "create_edge".to_string(),
                description: "Create a relationship between two nodes".to_string(),
                parameters: vec![
                    ToolParameter {
                        name: "subject_id".to_string(),
                        description: "The UUID of the subject node".to_string(),
                        param_type: "string".to_string(),
                        required: true,
                    },
                    ToolParameter {
                        name: "predicate".to_string(),
                        description: "The relationship type (e.g., WORKS_ON, USES)".to_string(),
                        param_type: "string".to_string(),
                        required: true,
                    },
                    ToolParameter {
                        name: "object_id".to_string(),
                        description: "The UUID of the object node".to_string(),
                        param_type: "string".to_string(),
                        required: true,
                    },
                ],
                can_write: true,
            });
        }

        Self {
            tools,
            graph,
            write_enabled,
        }
    }

    pub fn tools(&self) -> &[Tool] {
        &self.tools
    }

    pub fn graph(&self) -> Arc<parking_lot::RwLock<Graph>> {
        Arc::clone(&self.graph)
    }

    pub fn execute(&self, call: ToolCall) -> ToolResult {
        match call.tool_name.as_str() {
            "query_graph" => self.execute_query_graph(call.arguments),
            "list_all_nodes" => self.execute_list_all_nodes(call.arguments),
            "get_node_details" => self.execute_get_node_details(call.arguments),
            "list_directory" => self.execute_list_directory(call.arguments),
            "read_file" => self.execute_read_file(call.arguments),
            "search_code" => self.execute_search_code(call.arguments),
            "search_subgraph" => self.execute_search_subgraph(call.arguments),
            "upsert_node" if self.write_enabled => self.execute_upsert_node(call.arguments),
            "delete_node" if self.write_enabled => self.execute_delete_node(call.arguments),
            "adjust_weight" if self.write_enabled => self.execute_adjust_weight(call.arguments),
            "create_edge" if self.write_enabled => self.execute_create_edge(call.arguments),
            _ => ToolResult {
                success: false,
                output: serde_json::Value::Null,
                error: Some(format!(
                    "Unknown tool or write not enabled: {}",
                    call.tool_name
                )),
            },
        }
    }

    fn execute_list_all_nodes(&self, _args: HashMap<String, serde_json::Value>) -> ToolResult {
        let g = self.graph.read();
        let nodes: Vec<_> = g
            .drafts
            .values()
            .map(|n| {
                serde_json::json!({
                    "id": n.id,
                    "label": n.label,
                    "tier": format!("{:?}", n.tier),
                    "confidence": n.confidence.as_u8()
                })
            })
            .collect();

        ToolResult {
            success: true,
            output: serde_json::json!({"nodes": nodes, "count": nodes.len()}),
            error: None,
        }
    }

    fn execute_get_node_details(&self, args: HashMap<String, serde_json::Value>) -> ToolResult {
        let node_id = args.get("node_id").and_then(|v| v.as_str()).unwrap_or("");

        if let Ok(uuid) = uuid::Uuid::parse_str(node_id) {
            let g = self.graph.read();
            if let Some(node) = g.drafts.get(&uuid) {
                return ToolResult {
                    success: true,
                    output: serde_json::json!({
                        "id": node.id,
                        "label": node.label,
                        "tier": format!("{:?}", node.tier),
                        "confidence": node.confidence.as_u8(),
                        "properties": node.properties,
                        "created_at": node.created_at.to_rfc3339()
                    }),
                    error: None,
                };
            }
        }

        ToolResult {
            success: false,
            output: serde_json::Value::Null,
            error: Some(format!("Node not found: {}", node_id)),
        }
    }

    fn execute_list_directory(&self, args: HashMap<String, serde_json::Value>) -> ToolResult {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");

        let entries = match std::fs::read_dir(path) {
            Ok(dir) => dir
                .filter_map(|e| e.ok())
                .map(|e| {
                    let file_type = e
                        .file_type()
                        .map(|ft| {
                            if ft.is_dir() {
                                "directory"
                            } else if ft.is_file() {
                                "file"
                            } else {
                                "other"
                            }
                        })
                        .unwrap_or("unknown");

                    serde_json::json!({
                        "name": e.file_name().to_string_lossy(),
                        "type": file_type
                    })
                })
                .collect::<Vec<_>>(),
            Err(e) => {
                return ToolResult {
                    success: false,
                    output: serde_json::Value::Null,
                    error: Some(format!("Failed to read directory: {}", e)),
                }
            }
        };

        ToolResult {
            success: true,
            output: serde_json::json!({"entries": entries}),
            error: None,
        }
    }

    fn execute_read_file(&self, args: HashMap<String, serde_json::Value>) -> ToolResult {
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or("");

        if path.is_empty() {
            return ToolResult {
                success: false,
                output: serde_json::Value::Null,
                error: Some("Path is required".to_string()),
            };
        }

        match std::fs::read_to_string(path) {
            Ok(content) => {
                let len = content.len();
                let truncated = if len > 5000 {
                    format!("{}...\n(truncated {} chars)", &content[..5000], len - 5000)
                } else {
                    content
                };

                ToolResult {
                    success: true,
                    output: serde_json::json!({"content": truncated, "length": len}),
                    error: None,
                }
            }
            Err(e) => ToolResult {
                success: false,
                output: serde_json::Value::Null,
                error: Some(format!("Failed to read file: {}", e)),
            },
        }
    }

    fn execute_search_code(&self, args: HashMap<String, serde_json::Value>) -> ToolResult {
        let pattern = args.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
        let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");

        if pattern.is_empty() {
            return ToolResult {
                success: false,
                output: serde_json::Value::Null,
                error: Some("Pattern is required".to_string()),
            };
        }

        let regex = match regex::Regex::new(pattern) {
            Ok(r) => r,
            Err(e) => {
                return ToolResult {
                    success: false,
                    output: serde_json::Value::Null,
                    error: Some(format!("Invalid regex: {}", e)),
                }
            }
        };

        let mut results = Vec::new();

        fn search_dir(
            dir: &std::path::Path,
            pattern: &regex::Regex,
            results: &mut Vec<serde_json::Value>,
        ) {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.filter_map(|e| e.ok()) {
                    let path = entry.path();
                    if path.is_dir() {
                        if let Some(name) = path.file_name().map(|n| n.to_string_lossy()) {
                            if !name.starts_with('.') && name != "target" && name != "node_modules"
                            {
                                search_dir(&path, pattern, results);
                            }
                        }
                    } else if let Some(ext) = path.extension() {
                        let ext_str = ext.to_string_lossy();
                        if ["rs", "json", "toml", "md", "txt", "yaml", "yml"]
                            .contains(&ext_str.as_ref())
                        {
                            if let Ok(content) = std::fs::read_to_string(&path) {
                                for (i, line) in content.lines().enumerate() {
                                    if pattern.is_match(line) {
                                        results.push(serde_json::json!({
                                            "file": path.to_string_lossy(),
                                            "line": i + 1,
                                            "content": line
                                        }));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        search_dir(std::path::Path::new(path), &regex, &mut results);

        ToolResult {
            success: true,
            output: serde_json::json!({"matches": results, "count": results.len()}),
            error: None,
        }
    }

    fn execute_query_graph(&self, args: HashMap<String, serde_json::Value>) -> ToolResult {
        let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");

        let tier = args
            .get("tier")
            .and_then(|v| v.as_str())
            .and_then(|s| match s {
                "gold" => Some(Tier::Gold),
                "draft" => Some(Tier::Draft),
                "unverified" => Some(Tier::Unverified),
                _ => None,
            });

        let graph = self.graph.read();
        let results = graph.search(query, tier);

        ToolResult {
            success: true,
            output: serde_json::json!({
                "nodes": results.iter().map(|n| {
                    serde_json::json!({
                        "id": n.id,
                        "label": n.label,
                        "properties": n.properties,
                        "confidence": n.confidence.as_u8(),
                        "tier": format!("{:?}", n.tier)
                    })
                }).collect::<Vec<_>>()
            }),
            error: None,
        }
    }

    fn execute_search_subgraph(&self, args: HashMap<String, serde_json::Value>) -> ToolResult {
        let subgraph_id = match args.get("subgraph_id").and_then(|v| v.as_str()) {
            Some(s) => match uuid::Uuid::parse_str(s) {
                Ok(id) => id,
                Err(_) => {
                    return ToolResult {
                        success: false,
                        output: serde_json::Value::Null,
                        error: Some("Invalid UUID format".to_string()),
                    }
                }
            },
            None => {
                return ToolResult {
                    success: false,
                    output: serde_json::Value::Null,
                    error: Some("subgraph_id is required".to_string()),
                }
            }
        };

        let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");

        let graph = self.graph.read();
        let results = graph.subgraphs.get(&subgraph_id).map(|sg| {
            sg.find_nodes_by_label(query)
                .into_iter()
                .cloned()
                .collect::<Vec<_>>()
        });

        match results {
            Some(nodes) => ToolResult {
                success: true,
                output: serde_json::json!({
                    "nodes": nodes.iter().map(|n| {
                        serde_json::json!({
                            "id": n.id,
                            "label": n.label,
                            "properties": n.properties
                        })
                    }).collect::<Vec<_>>()
                }),
                error: None,
            },
            None => ToolResult {
                success: false,
                output: serde_json::Value::Null,
                error: Some("Subgraph not found".to_string()),
            },
        }
    }

    fn execute_upsert_node(&self, args: HashMap<String, serde_json::Value>) -> ToolResult {
        let label = match args.get("label").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => {
                return ToolResult {
                    success: false,
                    output: serde_json::Value::Null,
                    error: Some("label is required".to_string()),
                }
            }
        };

        let properties: std::collections::HashMap<String, serde_json::Value> = args
            .get("properties")
            .and_then(|v| v.as_object())
            .map(|m| m.clone().into_iter().collect())
            .unwrap_or_default();

        let confidence = args
            .get("confidence")
            .and_then(|v| v.as_u64())
            .map(|v| crate::graph::ConfidenceScore::new(v as u8))
            .unwrap_or_default();

        let mut node = Node::new(label);
        node.properties = properties;
        let node = node.with_confidence(confidence);

        let mut graph = self.graph.write();
        let id = graph.add_to_drafts(node);

        ToolResult {
            success: true,
            output: serde_json::json!({
                "node_id": id,
                "message": "Node created in drafts"
            }),
            error: None,
        }
    }

    fn execute_delete_node(&self, args: HashMap<String, serde_json::Value>) -> ToolResult {
        let node_id = match args.get("node_id").and_then(|v| v.as_str()) {
            Some(s) => match uuid::Uuid::parse_str(s) {
                Ok(id) => id,
                Err(_) => {
                    return ToolResult {
                        success: false,
                        output: serde_json::Value::Null,
                        error: Some("Invalid UUID format".to_string()),
                    }
                }
            },
            None => {
                return ToolResult {
                    success: false,
                    output: serde_json::Value::Null,
                    error: Some("node_id is required".to_string()),
                }
            }
        };

        let mut graph = self.graph.write();
        let mut found = false;

        for subgraph in graph.subgraphs.values_mut() {
            if subgraph.nodes.remove(&node_id).is_some() {
                found = true;
                break;
            }
        }

        if !found {
            graph.drafts.remove(&node_id);
        }

        ToolResult {
            success: true,
            output: serde_json::json!({
                "node_id": node_id,
                "message": "Node deleted"
            }),
            error: None,
        }
    }

    fn execute_adjust_weight(&self, args: HashMap<String, serde_json::Value>) -> ToolResult {
        let node_id = match args.get("node_id").and_then(|v| v.as_str()) {
            Some(s) => match uuid::Uuid::parse_str(s) {
                Ok(id) => id,
                Err(_) => {
                    return ToolResult {
                        success: false,
                        output: serde_json::Value::Null,
                        error: Some("Invalid UUID format".to_string()),
                    }
                }
            },
            None => {
                return ToolResult {
                    success: false,
                    output: serde_json::Value::Null,
                    error: Some("node_id is required".to_string()),
                }
            }
        };

        let confidence = match args.get("confidence").and_then(|v| v.as_u64()) {
            Some(v) => crate::graph::ConfidenceScore::new(v as u8),
            None => {
                return ToolResult {
                    success: false,
                    output: serde_json::Value::Null,
                    error: Some("confidence is required".to_string()),
                }
            }
        };

        let mut graph = self.graph.write();
        let mut found = false;

        for subgraph in graph.subgraphs.values_mut() {
            if let Some(node) = subgraph.nodes.get_mut(&node_id) {
                node.update_confidence(confidence);
                found = true;
                break;
            }
        }

        if !found {
            if let Some(node) = graph.drafts.get_mut(&node_id) {
                node.update_confidence(confidence);
                found = true;
            }
        }

        ToolResult {
            success: found,
            output: serde_json::json!({
                "node_id": node_id,
                "confidence": confidence.as_u8(),
                "tier": format!("{:?}", confidence.tier())
            }),
            error: None,
        }
    }

    fn execute_create_edge(&self, args: HashMap<String, serde_json::Value>) -> ToolResult {
        let subject_id = match args.get("subject_id").and_then(|v| v.as_str()) {
            Some(s) => match uuid::Uuid::parse_str(s) {
                Ok(id) => id,
                Err(_) => {
                    return ToolResult {
                        success: false,
                        output: serde_json::Value::Null,
                        error: Some("Invalid subject_id UUID format".to_string()),
                    }
                }
            },
            None => {
                return ToolResult {
                    success: false,
                    output: serde_json::Value::Null,
                    error: Some("subject_id is required".to_string()),
                }
            }
        };

        let predicate = match args.get("predicate").and_then(|v| v.as_str()) {
            Some(s) => s.to_string(),
            None => {
                return ToolResult {
                    success: false,
                    output: serde_json::Value::Null,
                    error: Some("predicate is required".to_string()),
                }
            }
        };

        let object_id = match args.get("object_id").and_then(|v| v.as_str()) {
            Some(s) => match uuid::Uuid::parse_str(s) {
                Ok(id) => id,
                Err(_) => {
                    return ToolResult {
                        success: false,
                        output: serde_json::Value::Null,
                        error: Some("Invalid object_id UUID format".to_string()),
                    }
                }
            },
            None => {
                return ToolResult {
                    success: false,
                    output: serde_json::Value::Null,
                    error: Some("object_id is required".to_string()),
                }
            }
        };

        let edge = Edge::new(subject_id, predicate, object_id);
        let edge_id = edge.id;

        let mut graph = self.graph.write();

        let mut added = false;
        for subgraph in graph.subgraphs.values_mut() {
            if subgraph.nodes.contains_key(&subject_id) && subgraph.nodes.contains_key(&object_id) {
                subgraph.add_edge(edge);
                added = true;
                break;
            }
        }

        ToolResult {
            success: added,
            output: serde_json::json!({
                "edge_id": edge_id,
                "message": if added { "Edge created" } else { "Could not find nodes for edge" }
            }),
            error: None,
        }
    }
}
