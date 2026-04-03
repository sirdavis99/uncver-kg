use crate::pipeline::AgentPipeline;
use crate::storage::Storage;
use crate::tools::ToolRegistry;
use std::sync::Arc;

pub struct ChatUI {
    pipeline: AgentPipeline,
    storage: Storage,
}

impl ChatUI {
    pub fn new(pipeline: AgentPipeline, storage: Storage) -> Self {
        Self { pipeline, storage }
    }

    pub async fn run(&self) -> anyhow::Result<SessionStats> {
        let spinner_frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
        let mut spinner_idx = 0;
        let mut stats = SessionStats::default();

        loop {
            print!("\n❯ ");
            std::io::Write::flush(&mut std::io::stdout()).unwrap();
            
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            let input = input.trim();
            
            if input.is_empty() {
                continue;
            }
            
            // Handle commands
            match input {
                "exit" | "quit" | "q" => {
                    println!("\n┌─────────────────────────────────────────────┐");
                    println!("│  👋 Session ended                           │");
                    println!("└─────────────────────────────────────────────┘");
                    println!("  Total queries: {}", stats.total_queries);
                    println!("  Nodes learned:  {}", stats.nodes_learned);
                    println!("  Nodes read:     {}", stats.nodes_read);
                    break;
                }
                
                "stats" | "/stats" => {
                    Self::print_session_stats(&stats);
                    continue;
                }
                
                "todo" | "/todo" => {
                    println!("\n  (Todo integration coming soon)");
                    continue;
                }
                
                "help" | "/help" | "?" => {
                    Self::print_help();
                    continue;
                }
                
                _ => {}
            }
            
            stats.total_queries += 1;
            
            // Query with spinner
            let response = Self::run_with_spinner(
                self.pipeline.clone(), 
                input, 
                &mut spinner_idx, 
                &spinner_frames
            );
            Self::print_response(&response);
            
            // Learning
            self.run_learning(input, &response, &mut stats).await;
        }
        
        Ok(stats)
    }

    fn run_with_spinner(
        pipeline: AgentPipeline,
        input: &str,
        spinner_idx: &mut usize,
        frames: &[&str],
    ) -> String {
        let (tx, rx) = std::sync::mpsc::channel();
        let input_owned = input.to_string();
        
        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let result = rt.block_on(async {
                pipeline.query(&input_owned).await
            });
            let _ = tx.send(result);
        });
        
        print!("  ");
        std::io::Write::flush(&mut std::io::stdout()).unwrap();
        
        loop {
            if let Ok(resp) = rx.try_recv() {
                return resp;
            }
            print!("\r  {} ", frames[*spinner_idx % frames.len()]);
            std::io::Write::flush(&mut std::io::stdout()).unwrap();
            *spinner_idx += 1;
            std::thread::sleep(std::time::Duration::from_millis(80));
        }
    }

    async fn run_learning(&self, input: &str, response: &str, stats: &mut SessionStats) {
        // Fire-and-forget: learning happens silently in background
        let input_learn = input.to_string();
        let response_learn = response.to_string();
        let pipeline_learn = self.pipeline.clone();
        
        tokio::spawn(async move {
            let _ = pipeline_learn.query_with_learn(&input_learn, &response_learn).await;
        });
        
        // Don't wait or show any output - just update stats if we happen to know
        stats.nodes_learned += 1;
    }

    fn print_session_stats(stats: &SessionStats) {
        println!("\n┌─────────────────────────────────────────────┐");
        println!("│  📊 Session Stats                           │");
        println!("└─────────────────────────────────────────────┘");
        println!("  ❯ Total queries: {}", stats.total_queries);
        println!("  ❯ Nodes learned:  {}", stats.nodes_learned);
        println!("  ❯ Nodes read:     {}", stats.nodes_read);
    }

    fn print_help() {
        println!("\n┌─────────────────────────────────────────────┐");
        println!("│  📖 Commands                               │");
        println!("└─────────────────────────────────────────────┘");
        println!("  exit/quit    - Exit chat");
        println!("  stats        - Show session stats");
        println!("  todo         - Show todo list");
        println!("  help         - Show this help");
    }

    fn print_response(response: &str) {
        println!("\r  ");
        println!("\n┌─────────────────────────────────────────────┐");
        println!("│  🤖 Bot                                    │");
        println!("└─────────────────────────────────────────────┘");
        for line in response.lines().take(10) {
            println!("  {}", line);
        }
        if response.lines().count() > 10 {
            println!("  ... ({} more lines)", response.lines().count() - 10);
        }
    }
}

#[derive(Default)]
pub struct SessionStats {
    pub total_queries: u32,
    pub nodes_learned: u32,
    pub nodes_read: u32,
}