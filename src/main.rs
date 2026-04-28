use clap::Parser;
use std::process;

/// Static code analyzer for TypeScript projects.
#[derive(Parser)]
#[command(name = "statico", version, about = "Static code analyzer for TypeScript projects")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(clap::Subcommand)]
enum Commands {
    /// Analyze a TypeScript project and output structured JSON.
    Analyze {
        /// Path to the TypeScript project directory.
        path: String,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Analyze { path } => {
            let root = std::path::Path::new(&path);
            // Canonicalize so that resolved import paths can be made relative to root.
            let root = match std::fs::canonicalize(root) {
                Ok(c) => c,
                Err(_) => root.to_path_buf(),
            };
            match statico::analyzer::analyze(&root) {
                Ok(output) => {
                    let json = serde_json::to_string_pretty(&output)
                        .unwrap_or_else(|e| {
                            eprintln!("error: failed to serialize output: {}", e);
                            process::exit(1);
                        });
                    println!("{}", json);
                }
                Err(msg) => {
                    eprintln!("error: {}", msg);
                    process::exit(1);
                }
            }
        }
    }
}
