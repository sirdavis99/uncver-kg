use crate::graph::{Graph, MainNetwork, SubGraph};
use anyhow::Result;
use parking_lot::RwLock;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, info};

#[derive(Clone)]
pub struct Storage {
    base_path: PathBuf,
    graph: Arc<RwLock<Graph>>,
}

impl Storage {
    pub fn new(base_path: impl Into<PathBuf>) -> Result<Self> {
        let base_path = base_path.into();

        if !base_path.exists() {
            std::fs::create_dir_all(&base_path)?;
        }

        let subgraphs_dir = base_path.join("subgraphs");
        if !subgraphs_dir.exists() {
            std::fs::create_dir_all(&subgraphs_dir)?;
        }

        let drafts_dir = base_path.join("drafts");
        if !drafts_dir.exists() {
            std::fs::create_dir_all(&drafts_dir)?;
        }

        // Setup persistent file logging
        let log_path = base_path.join("uncverkg.log");
        if let Ok(file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
        {
            use std::sync::Mutex;
            let _ = tracing_subscriber::fmt()
                .with_writer(Mutex::new(file))
                .with_ansi(false)
                .with_target(true)
                .with_thread_ids(true)
                .try_init();
        }

        info!("[STORAGE] Initialized at {:?}", base_path);

        let graph = Self::load_or_create(&base_path)?;
        info!(
            "[STORAGE] Loaded graph: {} subgraphs, {} drafts",
            graph.subgraphs.len(),
            graph.drafts.len()
        );

        Ok(Self {
            base_path,
            graph: Arc::new(RwLock::new(graph)),
        })
    }

    fn load_or_create(base_path: &Path) -> Result<Graph> {
        let main_network_path = base_path.join("main_network.json");

        if main_network_path.exists() {
            let main_network: MainNetwork =
                serde_json::from_str(&std::fs::read_to_string(&main_network_path)?)?;

            let mut graph = Graph::new();
            graph.main_network = main_network;

            let subgraphs_dir = base_path.join("subgraphs");
            if subgraphs_dir.exists() {
                for entry in std::fs::read_dir(subgraphs_dir)? {
                    let entry = entry?;
                    if entry.path().extension().map_or(false, |e| e == "json") {
                        let subgraph: SubGraph =
                            serde_json::from_str(&std::fs::read_to_string(entry.path())?)?;
                        graph.subgraphs.insert(subgraph.id, subgraph);
                    }
                }
            }

            let drafts_dir = base_path.join("drafts");
            if drafts_dir.exists() {
                for entry in std::fs::read_dir(drafts_dir)? {
                    let entry = entry?;
                    if entry.path().extension().map_or(false, |e| e == "json") {
                        let node: crate::graph::Node =
                            serde_json::from_str(&std::fs::read_to_string(entry.path())?)?;
                        graph.drafts.insert(node.id, node);
                    }
                }
            }

            Ok(graph)
        } else {
            let graph = Graph::new();
            let main_network_json = serde_json::to_string_pretty(&graph.main_network)?;
            std::fs::write(&main_network_path, main_network_json)?;
            Ok(graph)
        }
    }

    pub fn save_main_network(&self) -> Result<()> {
        let graph = self.graph.read();
        let path = self.base_path.join("main_network.json");
        let json = serde_json::to_string_pretty(&graph.main_network)?;
        std::fs::write(path, json)?;
        info!(
            "[STORAGE] Saved main_network.json with {} topics",
            graph.main_network.list_topics().len()
        );
        Ok(())
    }

    pub fn save_subgraph(&self, subgraph: &SubGraph) -> Result<()> {
        let path = self
            .base_path
            .join("subgraphs")
            .join(format!("{}.json", subgraph.id));
        let json = serde_json::to_string_pretty(subgraph)?;
        std::fs::write(path, json)?;
        info!(
            "[STORAGE] Saved subgraph '{}' (id: {}, nodes: {})",
            subgraph.name,
            subgraph.id,
            subgraph.nodes.len()
        );
        Ok(())
    }

    pub fn save_draft(&self, node: &crate::graph::Node) -> Result<()> {
        let path = self
            .base_path
            .join("drafts")
            .join(format!("{}.json", node.id));
        let json = serde_json::to_string_pretty(node)?;
        std::fs::write(path, json)?;
        debug!("[STORAGE] Saved draft node: {} ({})", node.label, node.id);
        Ok(())
    }

    pub fn delete_draft(&self, node_id: crate::graph::NodeId) -> Result<()> {
        let path = self
            .base_path
            .join("drafts")
            .join(format!("{}.json", node_id));
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    pub fn delete_subgraph(&self, subgraph_id: impl Into<uuid::Uuid>) -> Result<()> {
        let path = self
            .base_path
            .join("subgraphs")
            .join(format!("{}.json", subgraph_id.into()));
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    pub fn graph(&self) -> Arc<RwLock<Graph>> {
        Arc::clone(&self.graph)
    }

    pub fn base_path(&self) -> &Path {
        &self.base_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_storage_lifecycle() {
        let temp_dir = TempDir::new().unwrap();
        let storage = Storage::new(temp_dir.path()).unwrap();

        let graph = storage.graph();
        {
            let mut graph = graph.write();
            let _subgraph_id = graph.create_subgraph("test_topic".to_string());
            let node = crate::graph::Node::new("TestNode".to_string());
            graph.add_to_drafts(node);
        }

        storage.save_main_network().unwrap();
    }
}
