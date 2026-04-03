use clap::{Parser, Subcommand};
use kg_core::*;
use std::sync::Arc;
use std::collections::VecDeque;

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
    history: VecDeque<ChatMessage>,
    stats: SessionStats,
}

#[derive(Default, Clone)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Default)]
struct SessionStats {
    total_queries: u32,
    nodes_learned: u32,
    nodes_read: u32,
}

#[derive(Parser)]
#[command(name = "kg-core")]
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
    Init {
        #[arg(short, long)]
        topic: String,
    },
    Add {
        #[arg(short, long)]
        label: String,

        #[arg(short, long, default_value = "{}")]
        properties: String,

        #[arg(short, long)]
        confidence: Option<u8>,
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
    tracing_subscriber::fmt::init();

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
    } else {
        config.storage.base_path.clone()
    };

    let storage = Storage::new(&data_dir)?;
    let graph = storage.graph();

    match cli.command {
        Commands::Init { topic } => {
            let mut g = graph.write();
            let id = g.create_subgraph(topic.clone());
            println!("Created sub-graph '{}' with ID: {}", topic, id);
            drop(g);
            storage.save_main_network()?;
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
            
            let provider = providers::OllamaProvider::default_model("gemma2:2b");
            
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
                        tools: None,
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

        Commands::Chat { background } => {
            println!("Starting chat mode (background learning={})...", background);
            println!("Type 'exit' to quit, 'stats' to view session stats.\n");
            
            let mut session = ChatSession::default();
            
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

            loop {
                print!("You: ");
                std::io::Write::flush(&mut std::io::stdout()).unwrap();
                
                let mut input = String::new();
                std::io::stdin().read_line(&mut input).unwrap();
                let input = input.trim();
                
                if input.is_empty() {
                    continue;
                }
                
                if input == "exit" {
                    println!("\nGoodbye! Session stats:");
                    println!("  Total queries: {}", session.stats.total_queries);
                    println!("  Nodes learned:  {}", session.stats.nodes_learned);
                    println!("  Nodes read:     {}", session.stats.nodes_read);
                    break;
                }
                
                if input == "stats" {
                    println!("\n=== Session Stats ===");
                    println!("  Total queries: {}", session.stats.total_queries);
                    println!("  Nodes learned:  {}", session.stats.nodes_learned);
                    println!("  Nodes read:     {}", session.stats.nodes_read);
                    continue;
                }
                
                session.stats.total_queries += 1;
                
                let response = pipeline.query(input).await;
                println!("Bot: {}\n", response);
                
                if background {
                    pipeline.learn(input, &response);
                } else {
                    let result = pipeline.query_with_learn(input, &response).await;
                    if result {
                        session.stats.nodes_learned += 1;
                    }
                }
            }
        }

        Commands::Stats => {
            println!("\n=== Knowledge Graph Stats ===");
            let g = graph.read();
            
            println!("Main Network Topics: {}", g.main_network.list_topics().len());
            for topic in g.main_network.list_topics() {
                if let Some(sg_id) = g.main_network.find_subgraph(&topic) {
                    if let Some(sg) = g.subgraphs.get(sg_id) {
                        println!("  - {}: {} nodes, {} edges", topic, sg.node_count(), sg.edge_count());
                    }
                }
            }
            
            println!("Draft Nodes: {}", g.drafts.len());
            for node in g.drafts.values() {
                println!("  - {} [{}]", node.label, format!("{:?}", node.tier));
            }
            
            let total_nodes = g.subgraphs.values().map(|sg| sg.node_count() as u32).sum::<u32>() + g.drafts.len() as u32;
            let total_edges = g.subgraphs.values().map(|sg| sg.edge_count() as u32).sum::<u32>();
            println!("\nTotal nodes: {}", total_nodes);
            println!("Total edges: {}", total_edges);
        }
    }

    Ok(())
}
