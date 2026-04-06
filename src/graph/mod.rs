use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

pub type NodeId = Uuid;
pub type EdgeId = Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Tier {
    Draft,
    Unverified,
    Gold,
}

impl Tier {
    pub fn from_score(score: u8) -> Self {
        if score < 50 {
            Tier::Draft
        } else if score < 80 {
            Tier::Unverified
        } else {
            Tier::Gold
        }
    }

    pub fn threshold(&self) -> u8 {
        match self {
            Tier::Draft => 0,
            Tier::Unverified => 50,
            Tier::Gold => 80,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfidenceScore(u8);

impl ConfidenceScore {
    pub fn new(score: u8) -> Self {
        Self(score.min(100))
    }

    pub fn as_u8(&self) -> u8 {
        self.0
    }

    pub fn tier(&self) -> Tier {
        Tier::from_score(self.0)
    }

    pub fn from_signals(
        explicit_confirmation: bool,
        repetition_count: u8,
        logical_consistency: bool,
        model_certainty: bool,
    ) -> Self {
        let mut score = 0u8;
        if explicit_confirmation {
            score += 40;
        }
        score += (repetition_count.min(3) * 10).min(30);
        if logical_consistency {
            score += 20;
        }
        if model_certainty {
            score += 10;
        }
        Self(score.min(100))
    }
}

impl Default for ConfidenceScore {
    fn default() -> Self {
        Self(50)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub label: String,
    pub properties: HashMap<String, serde_json::Value>,
    pub confidence: ConfidenceScore,
    pub tier: Tier,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Node {
    pub fn new(label: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            label,
            properties: HashMap::new(),
            confidence: ConfidenceScore::default(),
            tier: Tier::Draft,
            created_at: now,
            updated_at: now,
        }
    }

    pub fn with_properties(mut self, properties: HashMap<String, serde_json::Value>) -> Self {
        self.properties = properties;
        self
    }

    pub fn with_confidence(mut self, confidence: ConfidenceScore) -> Self {
        self.confidence = confidence;
        self.tier = confidence.tier();
        self
    }

    pub fn promote(&mut self) {
        if self.tier != Tier::Gold {
            self.tier = Tier::Gold;
            self.confidence = ConfidenceScore::new(80);
        }
    }

    pub fn update_confidence(&mut self, score: ConfidenceScore) {
        self.confidence = score;
        self.tier = score.tier();
        self.updated_at = Utc::now();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub id: EdgeId,
    pub subject_id: NodeId,
    pub predicate: String,
    pub object_id: NodeId,
    pub confidence: ConfidenceScore,
    pub created_at: DateTime<Utc>,
}

impl Edge {
    pub fn new(subject_id: NodeId, predicate: String, object_id: NodeId) -> Self {
        Self {
            id: Uuid::new_v4(),
            subject_id,
            predicate,
            object_id,
            confidence: ConfidenceScore::default(),
            created_at: Utc::now(),
        }
    }

    pub fn with_confidence(mut self, confidence: ConfidenceScore) -> Self {
        self.confidence = confidence;
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubGraph {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub nodes: HashMap<NodeId, Node>,
    pub edges: HashMap<EdgeId, Edge>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl SubGraph {
    pub fn new(name: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            name,
            description: None,
            nodes: HashMap::new(),
            edges: HashMap::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn add_node(&mut self, node: Node) -> NodeId {
        let id = node.id;
        self.nodes.insert(id, node);
        self.updated_at = Utc::now();
        id
    }

    pub fn add_edge(&mut self, edge: Edge) -> EdgeId {
        let id = edge.id;
        self.edges.insert(id, edge);
        self.updated_at = Utc::now();
        id
    }

    pub fn get_node(&self, id: &NodeId) -> Option<&Node> {
        self.nodes.get(id)
    }

    pub fn get_node_mut(&mut self, id: &NodeId) -> Option<&mut Node> {
        self.nodes.get_mut(id)
    }

    pub fn find_nodes_by_label(&self, label: &str) -> Vec<&Node> {
        self.nodes
            .values()
            .filter(|n| n.label.to_lowercase().contains(&label.to_lowercase()))
            .collect()
    }

    pub fn find_nodes_by_tier(&self, tier: Tier) -> Vec<&Node> {
        self.nodes.values().filter(|n| n.tier == tier).collect()
    }

    pub fn find_edges_from(&self, node_id: NodeId) -> Vec<&Edge> {
        self.edges
            .values()
            .filter(|e| e.subject_id == node_id)
            .collect()
    }

    pub fn find_edges_to(&self, node_id: NodeId) -> Vec<&Edge> {
        self.edges
            .values()
            .filter(|e| e.object_id == node_id)
            .collect()
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MainNetwork {
    pub id: Uuid,
    pub topics: HashMap<String, Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl MainNetwork {
    pub fn new() -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4(),
            topics: HashMap::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn register_topic(&mut self, topic: String, subgraph_id: Uuid) {
        self.topics.insert(topic.to_lowercase(), subgraph_id);
        self.updated_at = Utc::now();
    }

    pub fn find_subgraph(&self, query: &str) -> Option<&Uuid> {
        let query_lower = query.to_lowercase();
        self.topics.get(&query_lower)
    }

    pub fn list_topics(&self) -> Vec<String> {
        self.topics.keys().cloned().collect()
    }

    pub fn to_graph_json(&self) -> String {
        let mut nodes = Vec::new();
        let mut edges = Vec::new();

        for (topic, sg_id) in &self.topics {
            nodes.push(serde_json::json!({
                "id": sg_id,
                "label": topic,
                "type": "topic"
            }));
        }

        for (i, topic1) in self.topics.keys().enumerate() {
            for topic2 in self.topics.keys().skip(i + 1) {
                edges.push(serde_json::json!({
                    "from": topic1,
                    "to": topic2,
                    "type": "related"
                }));
            }
        }

        serde_json::json!({
            "nodes": nodes,
            "edges": edges
        })
        .to_string()
    }
}

impl Default for MainNetwork {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Graph {
    pub main_network: MainNetwork,
    pub subgraphs: HashMap<Uuid, SubGraph>,
    pub drafts: HashMap<NodeId, Node>,
}

impl Graph {
    pub fn new() -> Self {
        Self {
            main_network: MainNetwork::new(),
            subgraphs: HashMap::new(),
            drafts: HashMap::new(),
        }
    }

    pub fn create_subgraph(&mut self, name: String) -> Uuid {
        let subgraph = SubGraph::new(name.clone());
        let id = subgraph.id;
        self.subgraphs.insert(id, subgraph);
        self.main_network.register_topic(name, id);
        id
    }

    pub fn add_to_drafts(&mut self, node: Node) -> NodeId {
        let id = node.id;
        self.drafts.insert(id, node);
        id
    }

    pub fn add_to_subgraph(&mut self, subgraph_id: Uuid, node: Node) -> Option<NodeId> {
        if let Some(subgraph) = self.subgraphs.get_mut(&subgraph_id) {
            let id = node.id;
            subgraph.add_node(node);
            Some(id)
        } else {
            None
        }
    }

    pub fn get_default_subgraph(&self) -> Option<&SubGraph> {
        self.subgraphs.values().next()
    }

    pub fn get_default_subgraph_mut(&mut self) -> Option<&mut SubGraph> {
        self.subgraphs.values_mut().next()
    }

    pub fn promote_draft(&mut self, node_id: NodeId, subgraph_id: Uuid) -> bool {
        if let Some(node) = self.drafts.remove(&node_id) {
            if let Some(subgraph) = self.subgraphs.get_mut(&subgraph_id) {
                subgraph.add_node(node);
                return true;
            }
            self.drafts.insert(node_id, node);
        }
        false
    }

    pub fn search(&self, query: &str, tier: Option<Tier>) -> Vec<&Node> {
        let mut results = Vec::new();

        for subgraph in self.subgraphs.values() {
            for node in subgraph.find_nodes_by_label(query) {
                if let Some(t) = tier {
                    if node.tier == t {
                        results.push(node);
                    }
                } else {
                    results.push(node);
                }
            }
        }

        for node in self.drafts.values() {
            if node.label.to_lowercase().contains(&query.to_lowercase()) {
                if let Some(t) = tier {
                    if node.tier == t {
                        results.push(node);
                    }
                } else {
                    results.push(node);
                }
            }
        }

        results
    }
}

impl Default for Graph {
    fn default() -> Self {
        Self::new()
    }
}
