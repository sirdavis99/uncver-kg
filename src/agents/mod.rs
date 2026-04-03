use crate::graph::{ConfidenceScore, Edge, Graph, Node, NodeId, SubGraph, Tier};
use crate::tools::{Tool, ToolCall, ToolRegistry};
use async_trait::async_trait;
use std::sync::Arc;

pub trait AgentMode: Send + Sync {
    fn can_write(&self) -> bool;
    fn get_system_prompt(&self) -> String;
    fn get_name(&self) -> &str;
}

pub struct Researcher;

impl AgentMode for Researcher {
    fn can_write(&self) -> bool {
        false
    }

    fn get_system_prompt(&self) -> String {
        r#"You are the Researcher. Your role is to search and retrieve relevant context from the knowledge graph.
        
Responsibilities:
- Search the graph for nodes and edges relevant to the user's query
- Focus on recall and finding accurate information
- Return a "Context Packet" containing facts and conversation history

Read Access:
- Gold tier (confidence > 80)
- Draft tier (confidence < 50)
- Unverified tier (confidence 50-80)

Tool Access:
- query_graph: Search for nodes by label or topic
- search_subgraph: Search within a specific sub-graph

Output format:
Return a JSON object with the search results formatted as context for the Actor."#.to_string()
    }

    fn get_name(&self) -> &str {
        "Researcher"
    }
}

pub struct Actor;

impl AgentMode for Actor {
    fn can_write(&self) -> bool {
        false
    }

    fn get_system_prompt(&self) -> String {
        r#"You are the Actor. Your role is to generate the final response to the user using the researched context.

Responsibilities:
- Use the context provided by the Researcher to answer the user
- Generate helpful, accurate responses
- Focus on utility and clarity

Read Access:
- Gold tier (confidence > 80)
- Draft tier (confidence < 50)
- Unverified tier (confidence 50-80)

Important:
- You have read-only access to the graph
- Do not attempt to modify or save any data
- If you need to verify information, request the Researcher to search again

Output format:
Generate a natural, helpful response to the user."#.to_string()
    }

    fn get_name(&self) -> &str {
        "Actor"
    }
}

pub struct Reviewer;

impl AgentMode for Reviewer {
    fn can_write(&self) -> bool {
        true
    }

    fn get_system_prompt(&self) -> String {
        r#"You are the Reviewer. Your role is to extract deductions, check for conflicts, and update the knowledge graph.

Responsibilities:
- Analyze the conversation to extract permanent, factual deductions
- Calculate confidence scores for new knowledge
- Check for conflicts with existing knowledge
- Update node weights or create new nodes

Confidence Scoring:
- Explicit confirmation from user: +40 points
- Repetition (mentioned 3+ times): +30 points
- Logical consistency with existing facts: +20 points
- High certainty language: +10 points

Promotion Logic:
- Score < 50: Keep in Drafts (temporary)
- Score 50-80: Add to Sub-Graph as Unverified
- Score > 80: Promote to Gold (permanent)

Conflict Resolution:
- If new fact contradicts existing Gold fact, create a Correction Log
- Prompt the user to confirm before overwriting

Read/Write Access:
- All tiers (Gold, Drafts, Unverified)
- Can create, update, and delete nodes

Tool Access:
- query_graph: Search existing knowledge
- upsert_node: Create or update a node
- delete_node: Remove a node
- adjust_weight: Change confidence score

Output format:
Return a JSON object containing:
1. Extracted deductions (array)
2. Confidence scores (per deduction)
3. Any conflicts found (array)
4. Actions taken (array)"#.to_string()
    }

    fn get_name(&self) -> &str {
        "Reviewer"
    }
}

#[derive(Debug, Clone)]
pub struct AgentContext {
    pub user_prompt: String,
    pub conversation_history: Vec<Message>,
    pub research_results: Option<Vec<Node>>,
    pub actor_response: Option<String>,
    pub deductions: Vec<Deduction>,
}

#[derive(Debug, Clone)]
pub struct Message {
    pub role: MessageRole,
    pub content: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone)]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone)]
pub struct Deduction {
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub confidence: ConfidenceScore,
    pub tier: Tier,
}

impl Deduction {
    pub fn to_node(&self) -> Node {
        let mut properties: std::collections::HashMap<String, serde_json::Value> =
            std::collections::HashMap::new();
        properties.insert("predicate".to_string(), serde_json::json!(self.predicate));
        properties.insert("object".to_string(), serde_json::json!(self.object));

        Node::new(self.subject.clone())
            .with_properties(properties)
            .with_confidence(self.confidence)
    }
}

pub struct Agent {
    mode: Box<dyn AgentMode>,
    tool_registry: Arc<ToolRegistry>,
    graph: Arc<parking_lot::RwLock<Graph>>,
}

impl Agent {
    pub fn new(
        mode: Box<dyn AgentMode>,
        tool_registry: Arc<ToolRegistry>,
        graph: Arc<parking_lot::RwLock<Graph>>,
    ) -> Self {
        Self {
            mode,
            tool_registry,
            graph,
        }
    }

    pub fn researcher(
        tool_registry: Arc<ToolRegistry>,
        graph: Arc<parking_lot::RwLock<Graph>>,
    ) -> Self {
        Self::new(Box::new(Researcher), tool_registry, graph)
    }

    pub fn actor(tool_registry: Arc<ToolRegistry>, graph: Arc<parking_lot::RwLock<Graph>>) -> Self {
        Self::new(Box::new(Actor), tool_registry, graph)
    }

    pub fn reviewer(
        tool_registry: Arc<ToolRegistry>,
        graph: Arc<parking_lot::RwLock<Graph>>,
    ) -> Self {
        Self::new(Box::new(Reviewer), tool_registry, graph)
    }

    pub fn can_write(&self) -> bool {
        self.mode.can_write()
    }

    pub fn system_prompt(&self) -> String {
        self.mode.get_system_prompt()
    }

    pub fn name(&self) -> &str {
        self.mode.get_name()
    }

    pub fn search(&self, query: &str, tier: Option<Tier>) -> Vec<Node> {
        let graph = self.graph.read();
        graph.search(query, tier).into_iter().cloned().collect()
    }

    pub fn search_subgraph(&self, subgraph_id: uuid::Uuid, query: &str) -> Option<Vec<Node>> {
        let graph = self.graph.read();
        graph
            .subgraphs
            .get(&subgraph_id)
            .map(|sg| sg.find_nodes_by_label(query).into_iter().cloned().collect())
    }

    pub fn add_node(&self, node: Node) -> Option<NodeId> {
        if !self.can_write() {
            return None;
        }
        let mut graph = self.graph.write();
        Some(graph.add_to_drafts(node))
    }

    pub fn update_node(&self, node_id: NodeId, confidence: ConfidenceScore) -> bool {
        if !self.can_write() {
            return false;
        }
        let mut graph = self.graph.write();

        for subgraph in graph.subgraphs.values_mut() {
            if let Some(node) = subgraph.nodes.get_mut(&node_id) {
                node.update_confidence(confidence);
                return true;
            }
        }

        if let Some(node) = graph.drafts.get_mut(&node_id) {
            node.update_confidence(confidence);
            return true;
        }

        false
    }

    pub fn delete_node(&self, node_id: NodeId) -> bool {
        if !self.can_write() {
            return false;
        }
        let mut graph = self.graph.write();

        for subgraph in graph.subgraphs.values_mut() {
            if subgraph.nodes.remove(&node_id).is_some() {
                return true;
            }
        }

        graph.drafts.remove(&node_id).is_some()
    }
}
