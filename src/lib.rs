pub mod agents;
pub mod chat;
pub mod config;
pub mod graph;
pub mod pipeline;
pub mod providers;
pub mod storage;
pub mod tools;

pub use chat::{ChatUI, SessionStats};
pub use graph::{ConfidenceScore, Edge, Graph, Node, NodeId, Tier};
pub use agents::{Actor, AgentMode, Researcher, Reviewer};
pub use config::Config;
pub use pipeline::AgentPipeline;
pub use providers::{LLMProvider, OllamaProvider};
pub use storage::Storage;
pub use tools::{Tool, ToolCall, ToolRegistry};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
