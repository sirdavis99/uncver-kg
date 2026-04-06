use clap::{Parser, Subcommand};
use kg_core::*;
use std::sync::Arc;
use std::collections::VecDeque;
use tracing::{info, error};

#[derive(Default)]
struct RunStats {
    researcher_calls: u32,
    actor_calls: u32,
    reviewer_calls: u32,
    tool_calls: u32,
    nodes_read: u32,
    nodes_created: u32,
    context_found: bool,
    researcher_tokens: u32,
    actor_tokens: u32,
}

#[derive(Default)]
struct ChatSession {
    #[allow(dead_code)]
    history: VecDeque<ChatMessage>,
    stats: SessionStats,
}

#[derive(Default, Clone)]
struct ChatMessage {
    #[allow(dead_code)]
    role: String,
    #[allow(dead_code)]
    content: String,
}

#[derive(Default)]
struct SessionStats {
    total_queries: u32,
    nodes_learned: u32,
    nodes_read: u32,
}

#[derive(Parser)]
#[command(name = "uncverkg")]
#[command(about = "Knowledge Graph Memory Engine CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    #[arg(short, long, default_value = "")]
    config: String,

    #[arg(short, long, default_value = "")]
    data_dir: String,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize current directory as an uncverkg project
    Init {
        /// Data directory name (unique per project)
        #[arg(short, long, default_value = "")]
        data_dir: String,
    },
    /// Create a new project folder
    Project {
        #[arg(short, long)]
        folder: String,
    },
    Add {
        #[arg(short, long)]
        label: String,

        #[arg(short, long, default_value = "{}")]
        properties: String,

        #[arg(short, long)]
        confidence: Option<u8>,
    },
    /// Read a node by ID or label
    Read {
        #[arg(short, long)]
        query: String,
    },
    /// Write a new fact/node
    Write {
        /// Subject of the fact
        #[arg(short, long)]
        subject: String,
        /// Predicate/relation
        #[arg(short, long)]
        predicate: String,
        /// Object of the fact
        #[arg(short, long)]
        object: String,
    },
    /// Bulk write facts from JSON
    Bulk {
        /// JSON file or inline JSON with facts array
        #[arg(short, long, default_value = "facts.json")]
        file: String,
    },
    /// Update an existing node
    Update {
        /// Node ID to update
        #[arg(short, long)]
        id: String,
        /// New label
        #[arg(short, long)]
        label: Option<String>,
        /// JSON properties to merge
        #[arg(short, long, default_value = "{}")]
        properties: String,
    },
    Search {
        #[arg(short, long)]
        query: String,

        #[arg(short, long)]
        tier: Option<String>,
    },
    List {
        #[arg(short, long)]
        topic: Option<String>,
    },
    Review {
        #[arg(short, long, default_value = "false")]
        confirm_all: bool,
    },
    Swap,
    Config {
        #[arg(short, long)]
        show: Option<String>,
    },
    Run {
        #[arg(short, long)]
        prompt: String,

        #[arg(short, long, default_value = "false")]
        verbose: bool,
    },
    Chat {
        #[arg(short, long, default_value = "true")]
        background: bool,
    },
    Stats,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Don't init default tracing - let storage handle it
    // tracing_subscriber::fmt::init(); 

    let cli = Cli::parse();

    let config = if !cli.config.is_empty() {
        Config::from_file(&cli.config)?
    } else if std::path::Path::new("config.json").exists() {
        Config::from_file("config.json")?
    } else {
        Config::default()
    };

    let data_dir = if !cli.data_dir.is_empty() {
        std::path::PathBuf::from(&cli.data_dir)
    } else if std::path::Path::new("kg.json").exists() {
        // Auto-detect: read data_dir from kg.json
        if let Ok(content) = std::fs::read_to_string("kg.json") {
            if let Ok(kg_config) = serde_json::from_str::<serde_json::Value>(&content) {
                if let Some(data_dir) = kg_config.get("data_dir").and_then(|v| v.as_str()) {
                    std::path::PathBuf::from(data_dir)
                } else {
                    std::path::PathBuf::from(".") // fallback
                }
            } else {
                std::path::PathBuf::from(".")
            }
        } else {
            std::path::PathBuf::from(".")
        }
    } else if std::path::Path::new("config.json").exists() {
        Config::from_file("config.json")?.storage.base_path.clone()
    } else {
        config.storage.base_path.clone()
    };

    let storage = Storage::new(&data_dir)?;
    let graph = storage.graph();

    match cli.command {
        Commands::Init { data_dir } => {
            let cwd = std::env::current_dir()
                .map(|p| p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default())
                .unwrap_or_default();
            
            let data_dir_name = if data_dir.is_empty() {
                format!("{}-kg", cwd)
            } else {
                data_dir
            };
            
            // Create kg.json in current directory
            let kg_config = format!(
                r#"{{
  "name": "{}",
  "version": "1.0.0",
  "kg_version": "0.1.0",
  "data_dir": "{}"
}}"#,
                cwd, data_dir_name
            );
            
            let kg_path = std::path::Path::new("kg.json");
            if kg_path.exists() {
                println!("⚠️  kg.json already exists. Skipping.");
            } else {
                std::fs::write(kg_path, kg_config)?;
                println!("✅ Created kg.json (data_dir: {})", data_dir_name);
            }
            
            // Create .gitignore or update (append data_dir)
            let gitignore_path = std::path::Path::new(".gitignore");
            let gitignore_content = format!("{}\n", data_dir_name);
            if gitignore_path.exists() {
                let existing = std::fs::read_to_string(gitignore_path)?;
                if !existing.contains(&data_dir_name) {
                    std::fs::write(gitignore_path, format!("{}\n{}", existing.trim(), gitignore_content))?;
                }
            } else {
                std::fs::write(gitignore_path, format!("{}\n", gitignore_content))?;
            }
            println!("✅ Updated .gitignore (excludes {})", data_dir_name);
            
            // Create facts/ directory inside data_dir
            let facts_dir = format!("{}/facts", data_dir_name);
            std::fs::create_dir_all(&facts_dir)?;
            println!("✅ Created {}/facts/ directory", data_dir_name);
            
            // Create SKILL.md
            let skill_content = format!(r#"# uncverkg Skill

## Purpose
Use this skill when you need to manage project knowledge using the uncverkg knowledge graph engine.

## Setup
- The project has `kg.json` in the root - uncverkg auto-detects this
- Knowledge is stored in `{0}/` (unique per project, gitignored)
- Bulk facts files go in `{0}/facts/`

## Commands

### Write a fact
```bash
uncverkg write --subject "Subject" --predicate "RELATION" --object "Object"
```

### Bulk write from JSON file
```bash
uncverkg bulk --file {0}/facts/my-facts.json
```

### Read/search nodes
```bash
uncverkg read --query "search term"
```

### Update a node
```bash
uncverkg update --id "UUID" --label "New Label"
```

### Start interactive chat with context
```bash
uncverkg chat
```

## Knowledge Graph Standard

### Node Format
Nodes are stored as facts: `Subject --PREDICATE--> Object`

### Properties
- `subject`: The entity being described
- `predicate`: The relationship/action
- `object`: The target of the relationship

### Example
```json
{{"subject": "David", "predicate": "WORKS_ON", "object": "Rust"}}
```

## When to Use
- User shares information about themselves or their project
- Learning new facts about code, architecture, or decisions
- Tracking project context across sessions
- Storing user preferences or settings
"#, data_dir_name);
            
            let skill_path = std::path::Path::new("SKILL.md");
            if !skill_path.exists() {
                std::fs::write(skill_path, skill_content)?;
                println!("✅ Created SKILL.md");
            }
            
            println!("\n✅ Initialized uncverkg in current directory!");
            println!("   Data stored in: {}/", data_dir_name);
        }

        Commands::Project { folder } => {
            let path = std::path::Path::new(&folder);
            let project_name = path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| folder.clone());
            
            // Data directory is unique per project: {project}-kg/
            let data_dir = format!("{}-kg", project_name);
            
            if path.exists() {
                println!("⚠️  Folder '{}' already exists", folder);
            } else {
                std::fs::create_dir_all(path)?;
                println!("✅ Created project folder: {}", folder);
            }
            
            // Create kg.json config in the folder (references the data directory)
            let kg_config = format!(
                r#"{{
  "name": "{}",
  "version": "1.0.0",
  "kg_version": "0.1.0",
  "data_dir": "{}"
}}"#,
                project_name, data_dir
            );
            let config_path = path.join("kg.json");
            std::fs::write(&config_path, kg_config)?;
            println!("✅ Created kg.json config (data_dir: {})", data_dir);
            
            // Create .gitignore (or update) to exclude the data directory
            let gitignore_path = path.join(".gitignore");
            let gitignore_content = format!("{}\n", data_dir);
            if gitignore_path.exists() {
                let existing = std::fs::read_to_string(&gitignore_path)?;
                if !existing.contains(&data_dir) {
                    std::fs::write(&gitignore_path, format!("{}\n{}", existing.trim(), gitignore_content))?;
                }
            } else {
                std::fs::write(&gitignore_path, gitignore_content)?;
            }
            println!("✅ Updated .gitignore (excludes {})", data_dir);
            
            // Create facts/ directory
            std::fs::create_dir_all(path.join("facts"))?;
            println!("✅ Created facts/ directory");
            
            // Create example facts file
            let example_facts = format!(r#"{{
  "facts": [
    {{"subject": "{}", "predicate": "IS_A", "object": "Project"}},
    {{"subject": "{}", "predicate": "BUILT_WITH", "object": "Language/Framework"}}
  ]
}}"#, project_name, project_name);
            std::fs::write(path.join("facts").join("example.json"), example_facts)?;
            println!("✅ Created facts/example.json");
            
            // Create skill file for AI agents
            let skill_content = format!(r#"# uncverkg Skill

## Purpose
Use this skill when you need to manage project knowledge using the uncverkg knowledge graph engine.

## Setup
- The project has `kg.json` in the root - uncverkg auto-detects this
- Knowledge is stored in `{}/` (unique per project, gitignored by default)

## Commands

### Write a fact
```bash
uncverkg write --subject "Subject" --predicate "RELATION" --object "Object"
```

### Bulk write from JSON file
```bash
uncverkg bulk --file facts/my-facts.json
```

### Read/search nodes
```bash
uncverkg read --query "search term"
```

### Update a node
```bash
uncverkg update --id "UUID" --label "New Label"
```

### Start interactive chat with context
```bash
uncverkg chat
```

## Knowledge Graph Standard

### Node Format
Nodes are stored as facts: `Subject --PREDICATE--> Object`

### Properties
- `subject`: The entity being described
- `predicate`: The relationship/action
- `object`: The target of the relationship

### Example
```json
{{"subject": "David", "predicate": "WORKS_ON", "object": "Rust"}}
```

## When to Use
- User shares information about themselves or their project
- Learning new facts about code, architecture, or decisions
- Tracking project context across sessions
- Storing user preferences or settings
"#, data_dir);
            std::fs::write(path.join("SKILL.md"), skill_content)?;
            println!("✅ Created SKILL.md for AI agents");
            
            println!("\n📁 Project '{}' initialized!", folder);
            println!("   cd {} && uncverkg chat  # Start chatting", folder);
            println!("   Data stored in: {}/", data_dir);
        }

        Commands::Read { query } => {
            let g = graph.read();
            
            // Try to find by ID first
            if let Ok(uuid) = uuid::Uuid::parse_str(&query) {
                // Check drafts
                if let Some(node) = g.drafts.get(&uuid) {
                    println!("\n📝 Node: {}", node.label);
                    println!("   ID: {}", node.id);
                    println!("   Tier: {:?}", node.tier);
                    println!("   Confidence: {}", node.confidence.as_u8());
                    println!("   Properties:");
                    for (k, v) in &node.properties {
                        println!("     {}: {}", k, v);
                    }
                    return Ok(());
                }
                // Check subgraphs
                for sg in g.subgraphs.values() {
                    if let Some(node) = sg.nodes.get(&uuid) {
                        println!("\n📝 Node: {}", node.label);
                        println!("   ID: {}", node.id);
                        println!("   Tier: {:?}", node.tier);
                        println!("   Confidence: {}", node.confidence.as_u8());
                        println!("   Properties:");
                        for (k, v) in &node.properties {
                            println!("     {}: {}", k, v);
                        }
                        return Ok(());
                    }
                }
            }
            
            // Search by label
            let results = g.search(&query, None);
            if results.is_empty() {
                println!("❌ No node found matching '{}'", query);
            } else {
                println!("\n🔍 Found {} node(s):", results.len());
                for node in results {
                    println!("   • {} (ID: {}, Tier: {:?})", node.label, node.id, node.tier);
                }
            }
        }

        Commands::Write { subject, predicate, object } => {
            let mut node = Node::new(format!("{} --{}--> {}", subject, predicate, object));
            node.properties.insert("subject".to_string(), serde_json::json!(subject));
            node.properties.insert("predicate".to_string(), serde_json::json!(predicate));
            node.properties.insert("object".to_string(), serde_json::json!(object));
            
            let mut g = graph.write();
            
            // Get or create default subgraph
            let subgraph_id = if g.subgraphs.is_empty() {
                g.create_subgraph("main".to_string())
            } else {
                g.subgraphs.keys().next().copied().unwrap()
            };
            
            let node_id = g.add_to_subgraph(subgraph_id, node).unwrap_or_else(|| {
                let n = Node::new(format!("{} --{}--> {}", subject, predicate, object));
                g.add_to_drafts(n)
            });
            
            drop(g);
            
            // Save
            if let Some(sg) = graph.read().subgraphs.get(&subgraph_id) {
                let _ = storage.save_subgraph(sg);
            }
            let _ = storage.save_main_network();
            
            println!("✅ Wrote: {} --{}--> {}", subject, predicate, object);
            println!("   Node ID: {}", node_id);
            println!("   💡 Learning happens automatically in background");
        }

        Commands::Bulk { file } => {
            // Try direct path first, then data_dir/facts/
            let file_path = if std::path::Path::new(&file).exists() {
                file.clone()
            } else {
                format!("{}/facts/{}", data_dir.display(), file)
            };
            
            let json_content = if std::path::Path::new(&file_path).exists() {
                std::fs::read_to_string(&file_path)?
            } else {
                file.clone() // Assume inline JSON
            };
            
            #[derive(serde::Deserialize)]
            struct Fact {
                subject: String,
                predicate: String,
                object: String,
            }
            
            #[derive(serde::Deserialize)]
            struct FactsInput {
                facts: Vec<Fact>,
            }
            
            let facts: Vec<Fact> = if let Ok(input) = serde_json::from_str::<FactsInput>(&json_content) {
                input.facts
            } else if let Ok(fact) = serde_json::from_str::<Fact>(&json_content) {
                vec![fact]
            } else {
                println!("❌ Invalid JSON format. Expected: {{\"facts\": [{{\"subject\": \"...\", \"predicate\": \"...\", \"object\": \"...\"}}]}}");
                return Ok(());
            };
            
            let mut g = graph.write();
            let subgraph_id = if g.subgraphs.is_empty() {
                g.create_subgraph("main".to_string())
            } else {
                g.subgraphs.keys().next().copied().unwrap()
            };
            
            let mut written = 0;
            for fact in &facts {
                let mut node = Node::new(format!("{} --{}--> {}", fact.subject, fact.predicate, fact.object));
                node.properties.insert("subject".to_string(), serde_json::json!(fact.subject));
                node.properties.insert("predicate".to_string(), serde_json::json!(fact.predicate));
                node.properties.insert("object".to_string(), serde_json::json!(fact.object));
                
                let _ = g.add_to_subgraph(subgraph_id, node);
                written += 1;
            }
            
            drop(g);
            
            // Save
            if let Some(sg) = graph.read().subgraphs.get(&subgraph_id) {
                let _ = storage.save_subgraph(sg);
            }
            let _ = storage.save_main_network();
            
            println!("✅ Wrote {} facts", written);
            println!("   💡 Learning happens automatically in background");
        }

        Commands::Update { id, label, properties } => {
            let uuid = match uuid::Uuid::parse_str(&id) {
                Ok(u) => u,
                Err(_) => {
                    println!("❌ Invalid UUID: {}", id);
                    return Ok(());
                }
            };
            
            let props: std::collections::HashMap<String, serde_json::Value> = 
                serde_json::from_str(&properties).unwrap_or_default();
            
            let mut g = graph.write();
            let mut updated = false;
            let label_ref = label.as_ref();
            
            // Check drafts
            if let Some(node) = g.drafts.get_mut(&uuid) {
                if let Some(l) = label_ref {
                    node.label = l.clone();
                }
                node.properties.extend(props.clone());
                node.updated_at = chrono::Utc::now();
                updated = true;
            }
            
            // Check subgraphs
            if !updated {
                for sg in g.subgraphs.values_mut() {
                    if let Some(node) = sg.nodes.get_mut(&uuid) {
                        if let Some(l) = label_ref {
                            node.label = l.clone();
                        }
                        node.properties.extend(props.clone());
                        node.updated_at = chrono::Utc::now();
                        updated = true;
                        break;
                    }
                }
            }
            
            drop(g);
            
            if updated {
                // Save
                for sg in graph.read().subgraphs.values() {
                    let _ = storage.save_subgraph(sg);
                }
                let _ = storage.save_main_network();
                println!("✅ Updated node: {}", id);
            } else {
                println!("❌ Node not found: {}", id);
            }
        }

        Commands::Add { label, properties, confidence } => {
            let props: std::collections::HashMap<String, serde_json::Value> = 
                serde_json::from_str(&properties).unwrap_or_default();
            
            let mut node = Node::new(label).with_properties(props);
            
            if let Some(c) = confidence {
                node = node.with_confidence(ConfidenceScore::new(c));
            }

            let mut g = graph.write();
            let id = g.add_to_drafts(node);
            println!("Added node with ID: {}", id);
            drop(g);
            
            storage.save_draft(
                graph.read().drafts.get(&id).unwrap()
            )?;
        }

        Commands::Search { query, tier } => {
            let tier_filter = tier.and_then(|t| match t.as_str() {
                "gold" => Some(Tier::Gold),
                "draft" => Some(Tier::Draft),
                "unverified" => Some(Tier::Unverified),
                _ => None,
            });

            let g = graph.read();
            let results = g.search(&query, tier_filter);
            
            println!("Found {} nodes:", results.len());
            for node in results {
                println!(
                    "  - {} [{}] (confidence: {})",
                    node.label,
                    format!("{:?}", node.tier),
                    node.confidence.as_u8()
                );
            }
        }

        Commands::List { topic } => {
            let g = graph.read();
            
            if let Some(t) = topic {
                if let Some(subgraph_id) = g.main_network.find_subgraph(&t) {
                    if let Some(sg) = g.subgraphs.get(subgraph_id) {
                        println!("Sub-graph '{}':", t);
                        println!("  Nodes: {}", sg.node_count());
                        println!("  Edges: {}", sg.edge_count());
                        
                        for node in sg.nodes.values() {
                            println!(
                                "    - {} [{}]",
                                node.label,
                                format!("{:?}", node.tier)
                            );
                        }
                    }
                } else {
                    println!("Topic '{}' not found", t);
                }
            } else {
                println!("Topics:");
                for topic in g.main_network.list_topics() {
                    println!("  - {}", topic);
                }
            }
        }

        Commands::Review { confirm_all } => {
            println!("Running review...");
            
            let _provider = providers::OllamaProvider::default_model("gemma2:2b");
            
            let g = graph.read();
            let drafts: Vec<_> = g.drafts.values().cloned().collect();
            drop(g);

            for node in drafts {
                println!("Reviewing node: {}", node.label);
                
                if confirm_all {
                    let mut g = graph.write();
                    if let Some(draft) = g.drafts.get_mut(&node.id) {
                        draft.promote();
                        println!("  Promoted to Gold");
                    }
                    drop(g);
                    
                    if let Some(updated) = graph.read().drafts.get(&node.id).cloned() {
                        storage.save_subgraph(
                            &graph.read().subgraphs.values().find(|sg| sg.nodes.contains_key(&node.id)).cloned().unwrap_or_else(|| {
                                let mut sg = graph::SubGraph::new("default".to_string());
                                sg.add_node(updated.clone());
                                sg
                            })
                        )?;
                    }
                }
            }
            
            storage.save_main_network()?;
            println!("Review complete.");
        }

        Commands::Swap => {
            println!("Performing Blue-Green swap...");
            storage.save_main_network()?;
            println!("Swap complete.");
        }

        Commands::Config { show } => {
            if let Some(key) = show {
                match key.as_str() {
                    "storage" => println!("{}", serde_json::to_string_pretty(&config.storage)?),
                    "llm" => println!("{}", serde_json::to_string_pretty(&config.llm)?),
                    "agents" => println!("{}", serde_json::to_string_pretty(&config.agents)?),
                    _ => println!("Unknown config key: {}", key),
                }
            } else {
                println!("{}", serde_json::to_string_pretty(&config)?);
            }
        }

        Commands::Run { prompt, verbose } => {
            if verbose {
                println!("Processing prompt: {}\n", prompt);
            }
            
            let tool_registry = Arc::new(tools::ToolRegistry::new(
                Arc::clone(&graph),
                true,
            ));

            let llm_model = config.llm.model.clone();
            let llm_base_url = config.llm.base_url.clone();
            
            if let Some(model) = llm_model {
                if !model.is_empty() {
                    let base_url = llm_base_url.unwrap_or_else(|| "http://localhost:11434".to_string());
                    let provider = providers::OllamaProvider::new(&base_url, &model);
                    let tools = Some(tool_registry.tools().to_vec());
                    
                    let mut stats = RunStats::default();
                    
                    // ============ RESEARCHER PHASE ============
                    if verbose {
                        println!("\n┌─────────────────────────────────────────────────────────────┐");
                        println!("│  🔍 RESEARCHER PHASE                                      │");
                        println!("└─────────────────────────────────────────────────────────────┘");
                    }
                    
                    let researcher_system = format!(
                        r#"You are the Researcher. Search the knowledge graph for what YOU have learned.

Query: "{}"

The knowledge graph contains what the AI has learned - including topics discussed AND info user shared.
Search for relevant facts, concepts, or user-shared information.
Return a brief summary of what you found."#,
                        prompt
                    );
                    
                    let researcher_msg = providers::Message {
                        role: "user".to_string(),
                        content: format!("Search for: {}", prompt),
                    };
                    
                    let researcher_request = providers::LLMRequest {
                        model: model.clone(),
                        messages: vec![
                            providers::Message { role: "system".to_string(), content: researcher_system },
                            researcher_msg,
                        ],
                        temperature: 0.3,
                        max_tokens: Some(512),
                        tools: tools.clone(),
                    };
                    
                    let researcher_result = provider.complete(researcher_request).await;
                    
                    if let Ok(res) = &researcher_result {
                        stats.researcher_calls += 1;
                        stats.researcher_tokens += res.content.len() as u32;
                        
                        if let Some(tcs) = &res.tool_calls {
                            stats.tool_calls += tcs.len() as u32;
                            if verbose {
                                for tc in tcs {
                                    println!("  📡 Tool call: {} - {:?}", tc.name, tc.arguments);
                                    let result = tool_registry.execute(tools::ToolCall {
                                        tool_name: tc.name.clone(),
                                        arguments: tc.arguments.clone(),
                                    });
                                    if result.success {
                                        stats.nodes_read += 1;
                                        println!("    ✓ Result: {}", result.output);
                                    }
                                }
                            } else {
                                for tc in tcs {
                                    tool_registry.execute(tools::ToolCall {
                                        tool_name: tc.name.clone(),
                                        arguments: tc.arguments.clone(),
                                    });
                                    stats.nodes_read += 1;
                                }
                            }
                        }
                        
                        if !res.content.is_empty() {
                            stats.context_found = true;
                            if verbose {
                                println!("\n  📊 Research context: {}", res.content);
                            }
                        }
                    } else if verbose {
                        println!("  ⚠ Researcher call failed");
                    }
                    
                    // ============ ACTOR PHASE ============
                    if verbose {
                        println!("\n┌─────────────────────────────────────────────────────────────┐");
                        println!("│  🎭 ACTOR PHASE                                           │");
                        println!("└─────────────────────────────────────────────────────────────┘");
                    }
                    
                    let researcher_context = researcher_result
                        .as_ref()
                        .map(|r| r.content.clone())
                        .unwrap_or_default();
                    
                    let actor_system = format!(
                        r#"You are a helpful AI assistant. Answer the user's question using the provided context.

User question: "{}"

Context from knowledge graph:
{}

Your response should:
- Be comprehensive and informative
- Use the context to provide specific details
- If context is empty, answer from your general knowledge"#,
                        prompt,
                        if researcher_context.is_empty() { "No prior knowledge found".to_string() } else { researcher_context }
                    );
                    
                    let actor_request = providers::LLMRequest {
                        model: model.clone(),
                        messages: vec![
                            providers::Message { role: "system".to_string(), content: actor_system },
                        ],
                        temperature: 0.7,
                        max_tokens: Some(1024),
                        tools: tools.clone(), // Give Actor access to tools too
                    };
                    
                    let actor_result = provider.complete(actor_request).await;
                    
                    if let Ok(res) = &actor_result {
                        stats.actor_calls += 1;
                        stats.actor_tokens += res.content.len() as u32;
                        if verbose {
                            println!("\n  💬 {}", res.content);
                        } else {
                            println!("{}", res.content);
                        }
                    }
                    
                    // ============ REVIEWER PHASE ============
                    if verbose {
                        println!("\n┌─────────────────────────────────────────────────────────────┐");
                        println!("│  📝 REVIEWER PHASE                                         │");
                        println!("└─────────────────────────────────────────────────────────────┘");
                    }
                    
                    let actor_response = actor_result
                        .as_ref()
                        .map(|r| r.content.clone())
                        .unwrap_or_default();
                    
                    let reviewer_system = format!(
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
                        prompt,
                        actor_response
                    );
                    
                    let reviewer_request = providers::LLMRequest {
                        model: model.clone(),
                        messages: vec![
                            providers::Message { role: "system".to_string(), content: reviewer_system },
                        ],
                        temperature: 0.3,
                        max_tokens: Some(512),
                        tools: tools.clone(),
                    };
                    
                    let reviewer_result = provider.complete(reviewer_request).await;
                    
                    if let Ok(res) = &reviewer_result {
                        stats.reviewer_calls += 1;
                        
                        if let Some(tcs) = &res.tool_calls {
                            for tc in tcs {
                                if verbose {
                                    println!("  📡 Tool call: {} - {:?}", tc.name, tc.arguments);
                                }
                                let result = tool_registry.execute(tools::ToolCall {
                                    tool_name: tc.name.clone(),
                                    arguments: tc.arguments.clone(),
                                });
                                if verbose {
                                    println!("    Result: {:?}", result.output);
                                }
                                if result.success {
                                    stats.nodes_created += 1;
                                }
                            }
                        }
                        
                        if !res.content.is_empty() && verbose {
                            println!("  📝 Reviewer response: {}", res.content);
                        }
                    }
                    
                    // Save all drafts after reviewer phase
                    for (_, draft) in graph.read().drafts.iter() {
                        let _ = storage.save_draft(draft);
                    }
                    
                    // Save all subgraphs after reviewer phase
                    for (_, subgraph) in graph.read().subgraphs.iter() {
                        let _ = storage.save_subgraph(subgraph);
                    }
                    
                    // Save main network
                    let _ = storage.save_main_network();
                    
                    // ============ STATS SUMMARY ============
                    if verbose {
                        println!("\n┌─────────────────────────────────────────────────────────────┐");
                        println!("│  📊 RUN STATISTICS                                        │");
                        println!("└─────────────────────────────────────────────────────────────┘");
                        println!("  Researcher calls:  {}", stats.researcher_calls);
                        println!("  Actor calls:        {}", stats.actor_calls);
                        println!("  Reviewer calls:     {}", stats.reviewer_calls);
                        println!("  Tool calls made:    {}", stats.tool_calls);
                        println!("  Nodes read:         {}", stats.nodes_read);
                        println!("  Nodes created:      {}", stats.nodes_created);
                        println!("  Context found:      {}", stats.context_found);
                        println!("  Researcher tokens:  ~{}", stats.researcher_tokens);
                        println!("  Actor tokens:       ~{}", stats.actor_tokens);
                    }
                } else {
                    println!("\n=== No LLM model configured ===");
                }
            } else {
                println!("\n=== No LLM model configured ===");
            }
        }

        Commands::Chat { background: _ } => {
            let llm_model = config.llm.model.clone();
            let llm_base_url = config.llm.base_url.clone();
            
            let (provider, model_name) = match llm_model {
                Some(m) if !m.is_empty() => {
                    let base_url = llm_base_url.unwrap_or_else(|| "http://localhost:11434".to_string());
                    (providers::OllamaProvider::new(&base_url, &m), m)
                }
                _ => {
                    println!("No LLM model configured. Please set model in config.json");
                    return Ok(());
                }
            };

            let tool_registry = Arc::new(tools::ToolRegistry::new(
                Arc::clone(&graph),
                true,
            ));
            
            let pipeline = pipeline::AgentPipeline::new(
                provider,
                tool_registry,
                storage.clone(),
                model_name,
            );

            println!("Starting chat mode...");
            println!("Type 'exit' to quit, 'help' for commands\n");
            
            let chat_ui = chat::ChatUI::new(pipeline, storage.clone());
            let _ = chat_ui.run().await;
        }

        Commands::Stats => {
            println!("\n┌─────────────────────────────────────────────┐");
            println!("│  📊 Knowledge Graph Stats                  │");
            println!("└─────────────────────────────────────────────┘");
            let g = graph.read();
            
            let subgraphs_count = g.subgraphs.len();
            let drafts_count = g.drafts.len();
            
            println!("\n  📁 Subgraphs: {}", subgraphs_count);
            if subgraphs_count > 0 {
                for sg in g.subgraphs.values() {
                    println!("     - {} ({} nodes)", sg.name, sg.node_count());
                }
            }
            
            println!("\n  📝 Draft nodes: {}", drafts_count);
        }
    }

    Ok(())
}
