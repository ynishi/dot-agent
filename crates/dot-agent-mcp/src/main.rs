use clap::Parser;

#[derive(Parser)]
#[command(
    name = "dot-agent-mcp",
    version,
    about = "MCP server for dot-agent profile management"
)]
struct Cli {
    /// Start as MCP server (stdio transport).
    #[arg(long)]
    mcp: bool,

    /// Start as MCP server (alias for --mcp, for Claude Code compatibility).
    #[arg(long)]
    stdio: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    if cli.mcp || cli.stdio {
        dot_agent_mcp::mcp::run().await
    } else {
        eprintln!("dot-agent-mcp: use --mcp or --stdio to start as MCP server");
        std::process::exit(1);
    }
}
