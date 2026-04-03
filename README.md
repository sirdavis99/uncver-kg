# uncverkg

Knowledge Graph Memory Engine - A Rust library for persistent LLM memory with 3-agent architecture.

## Overview

uncverkg implements a sophisticated memory system for LLMs with:
- **3-Agent Architecture**: Researcher (read), Actor (read), Reviewer (write)
- **Blue-Green Deployment**: Async graph updates with atomic swaps
- **Confidence Scoring**: Automatic knowledge quality assessment
- **Tiered Storage**: Gold (>80), Unverified (50-80), Draft (<50) tiers
- **Configurable LLM Providers**: Ollama, OpenAI, or programmatic fallback
- **File System Tools**: list_directory, read_file, search_code for AI exploration

## Installation

### Prerequisites
- Rust 1.75+
- (Optional) Ollama for LLM integration

### Via Homebrew
```bash
brew install sirdavis99/uncverkg/uncverkg
```

### Via Cargo
```bash
cargo install uncverkg
```

### Via Docker
```bash
docker build -t uncverkg .
docker run -v $(pwd)/data:/data uncverkg --help
```

## Usage

### CLI Commands

```bash
# Run a query (quiet mode - no verbose output)
uncverkg run --prompt "What do I know about Rust?"

# Run with verbose output (shows agent phases and stats)
uncverkg run --prompt "What do I know about Rust?" --verbose

# Interactive chat mode (background learning enabled by default)
uncverkg chat

# View knowledge graph stats
uncverkg stats

# Search the knowledge graph
uncverkg search --query "rust"

# Initialize a new topic subgraph
uncverkg init --topic "rust"

# Add a node to drafts
uncverkg add --label "David" --properties '{"role": "developer"}'

# List all topics
uncverkg list
```

### Available Tools for AI

The AI has access to these tools:

**Graph Tools:**
- `query_graph` - Search nodes by label/topic
- `list_all_nodes` - List all nodes in graph
- `get_node_details` - Get details of a specific node
- `upsert_node` - Create/update nodes (write mode)
- `create_edge` - Connect related nodes (write mode)

**File System Tools:**
- `list_directory` - List files in a directory
- `read_file` - Read file content
- `search_code` - Search text in files (grep)

## Architecture

```
uncverkg/
├── src/
│   ├── agents/      # 3-agent system (Researcher, Actor, Reviewer)
│   ├── graph/       # Core data structures (Node, Edge, Graph)
│   ├── tools/       # Tool registry and executions
│   ├── providers/   # LLM provider integrations
│   ├── storage/     # File-based persistence
│   ├── config/      # Configuration management
│   └── pipeline/    # Reusable 3-agent pipeline with async support
```

## Configuration

Create a `config.json`:

```json
{
  "storage": {
    "base_path": "./data",
    "auto_save": true
  },
  "llm": {
    "provider": "ollama",
    "model": "kimi-k2.5:cloud",
    "base_url": "http://localhost:11434",
    "temperature": 0.7
  },
  "agents": {
    "enable_researcher": true,
    "enable_actor": true,
    "enable_reviewer": true,
    "async_review": true
  }
}
```

## Development

```bash
# Build
cargo build --release

# Test
cargo test

# Clippy
cargo clippy -- -D warnings

# Format
cargo fmt --all
```

## License

MIT