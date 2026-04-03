use clap::{Parser, Subcommand};
use kg_core::*;
use std::sync::Arc;
use parking_lot::RwLock;

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
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    let config = if !cli.config.is_empty() {
        Config::from_file(&cli.config)?
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

        Commands::Run { prompt } => {
            println!("Processing prompt: {}", prompt);
            
            let tool_registry = Arc::new(tools::ToolRegistry::new(
                Arc::clone(&graph),
                true,
            ));

            let researcher = agents::Agent::researcher(
                Arc::clone(&tool_registry),
                Arc::clone(&graph),
            );
            
            println!("\n=== Researcher Phase ===");
            let results = researcher.search(&prompt, None);
            println!("Found {} relevant nodes", results.len());
            
            let actor = agents::Agent::actor(
                Arc::clone(&tool_registry),
                Arc::clone(&graph),
            );
            
            println!("\n=== Actor Phase ===");
            println!("System prompt: {}", actor.system_prompt());
            
            let reviewer = agents::Agent::reviewer(
                Arc::clone(&tool_registry),
                Arc::clone(&graph),
            );
            
            println!("\n=== Reviewer Phase ===");
            println!("System prompt: {}", reviewer.system_prompt());
            
            let node = Node::new("UserInput".to_string());
            if reviewer.add_node(node).is_some() {
                println!("Added user input to drafts");
            }
        }
    }

    Ok(())
}
