use crate::pipeline::AgentPipeline;
use crate::storage::Storage;

pub struct ChatUI {
    pipeline: AgentPipeline,
    storage: Storage,
}

impl ChatUI {
    pub fn new(pipeline: AgentPipeline, storage: Storage) -> Self {
        Self { pipeline, storage }
    }

    pub async fn run(&self) -> anyhow::Result<SessionStats> {
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

            // Run streaming chat - this blocks until streaming is done
            Self::run_streaming(&self.pipeline, input).await;

            // Learning happens in background
            self.run_learning(input).await;
        }

        Ok(stats)
    }

    async fn run_streaming(pipeline: &AgentPipeline, input: &str) {
        // Show bot header
        println!("\n┌─────────────────────────────────────────────────────┐");
        println!("│  🤖 Bot                                             │");
        println!("└─────────────────────────────────────────────────────┘");
        print!("  ");
        std::io::Write::flush(&mut std::io::stdout()).unwrap();

        // Stream directly - this blocks and prints as chunks arrive
        let _ = pipeline.query_streaming(input, |chunk, _tool_calls| {
            // Print each chunk as it arrives (true streaming)
            print!("{}", chunk);
            std::io::Write::flush(&mut std::io::stdout()).unwrap();
        }).await;

        println!(); // newline after streaming completes
    }

    async fn run_learning(&self, input: &str) {
        let input_learn = input.to_string();
        let pipeline_learn = self.pipeline.clone();

        tokio::spawn(async move {
            let _ = pipeline_learn.query_with_learn(&input_learn, "").await;
        });
    }

    fn print_session_stats(stats: &SessionStats) {
        println!("\n┌─────────────────────────────────────────────┐");
        println!("│  📊 Session Stats                           │");
        println!("└─────────────────────────────────────────────┘");
        println!("  ❯ Total queries: {}", stats.total_queries);
        println!("  ❯ Nodes learned: {}", stats.nodes_learned);
        println!("  ❯ Nodes read:    {}", stats.nodes_read);
    }

    fn print_help() {
        println!("\n┌─────────────────────────────────────────────┐");
        println!("│  📖 Commands                                │");
        println!("└─────────────────────────────────────────────┘");
        println!("  exit/quit - Exit chat");
        println!("  stats     - Show session stats");
        println!("  todo      - Show todo list");
        println!("  help      - Show this help");
    }
}

#[derive(Default)]
pub struct SessionStats {
    pub total_queries: u32,
    pub nodes_learned: u32,
    pub nodes_read: u32,
}
