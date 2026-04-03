# kg-core

Knowledge Graph Memory Engine - A Rust library for persistent LLM memory with 3-agent architecture.

## Overview

kg-core implements a sophisticated memory system for LLMs with:
- **3-Agent Architecture**: Researcher (read), Actor (read), Reviewer (write)
- **Blue-Green Deployment**: Async graph updates with atomic swaps
- **Confidence Scoring**: Automatic knowledge quality assessment
- **Tiered Storage**: Gold (>80), Unverified (50-80), Draft (<50) tiers
- **Configurable LLM Providers**: Ollama, OpenAI, or programmatic fallback

## Installation

### Via Homebrew
```bash
brew install yourusername/kg-core/kg-core
```

### Via Cargo
```bash
cargo install kg-core
```

### Via Docker
```bash
docker build -t kg-core .
docker run -v $(pwd)/data:/data kg-core --help
```

## Usage

### CLI Commands

```bash
# Initialize a new topic subgraph
kg-core init --topic "rust"

# Add a node to drafts
kg-core add --label "David" --properties '{"role": "developer"}'

# Search the knowledge graph
kg-core search --query "rust" --tier gold

# List all topics
kg-core list

# Run review to promote drafts
kg-core review --confirm-all

# Perform Blue-Green swap
kg-core swap
```

### Library Usage

```rust
use kg_core::*;

let storage = Storage::new("./data")?;
let graph = storage.graph();

// Create a subgraph
let subgraph_id = graph.write().create_subgraph("rust".to_string());

// Add nodes
let node = Node::new("David".to_string())
    .with_confidence(ConfidenceScore::new(90));
graph.write().add_to_drafts(node);

// Search
let results = graph.read().search("David", Some(Tier::Gold));
```

## Architecture

```
kg-core/
├── src/
│   ├── agents/      # 3-agent system (Researcher, Actor, Reviewer)
│   ├── graph/       # Core data structures (Node, Edge, Graph)
│   ├── tools/       # Tool registry and executions
│   ├── providers/   # LLM provider integrations
│   ├── storage/     # File-based persistence
│   └── config/      # Configuration management
```

## Configuration

Create a `config.yaml`:

```yaml
storage:
  base_path: "./data"
  auto_save: true

llm:
  provider: "ollama"
  model: "gemma2:2b"
  base_url: "http://localhost:11434"
  temperature: 0.7

agents:
  enable_researcher: true
  enable_actor: true
  enable_reviewer: true
  async_review: true
```

## Development

```bash
# Build
cargo build

# Test
cargo test

# Clippy
cargo clippy -- -D warnings

# Format
cargo fmt --all
```

## License

MIT
