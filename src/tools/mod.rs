use crate::graph::{Edge, Graph, Node, NodeId, Tier};
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
                name: "search_subgraph".to_string(),
                description: "Search within a specific sub-graph".to_string(),
                parameters: vec![
                    ToolParameter {
                        name: "subgraph_id".to_string(),
                        description: "The UUID of the sub-graph".to_string(),
                        param_type: "string".to_string(),
                        required: true,
                    },
                    ToolParameter {
                        name: "query".to_string(),
                        description: "The search query".to_string(),
                        param_type: "string".to_string(),
                        required: true,
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

    pub fn execute(&self, call: ToolCall) -> ToolResult {
        match call.tool_name.as_str() {
            "query_graph" => self.execute_query_graph(call.arguments),
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
