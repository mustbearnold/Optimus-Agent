//! `optimus children` — the CLI surface for recursive children
//! (spec-034 R7/R8): list the context tree of a session with the
//! attributed usage totals.
//!
//! Spawn, cancel, and delete are daemon surfaces (the kernel tools);
//! the CLI lists the durable registry and the usage attribution.

use clap::{Args, Subcommand};
use optimus_kernel::Kernel;

#[derive(Subcommand, Debug)]
pub enum ChildrenCmd {
    /// List direct children with status, depth, and attributed usage
    List,
}

#[derive(Args, Debug)]
pub struct ChildrenArgs {
    #[command(subcommand)]
    pub cmd: ChildrenCmd,
    /// Session uuid (default: a fresh session)
    #[arg(long, global = true)]
    pub session: Option<String>,
    /// Emit machine-readable JSON
    #[arg(long, global = true)]
    pub json: bool,
}

pub fn run_children(
    kernel: &mut Kernel,
    args: &ChildrenArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    match args.cmd {
        ChildrenCmd::List => list(kernel, args.json).map_err(Into::into),
    }
}

fn list(kernel: &Kernel, json: bool) -> Result<(), String> {
    let children = kernel
        .session_children_with_usage()
        .map_err(|e| e.to_string())?;
    if json {
        let payload = children
            .iter()
            .map(|child| {
                serde_json::json!({
                    "child_session_id": child.child_session_id.to_string(),
                    "depth": child.depth,
                    "status": child.status,
                    "total_tokens": child.total_tokens,
                    "input_tokens": child.input_tokens,
                    "output_tokens": child.output_tokens,
                    "reasoning_tokens": child.reasoning_tokens,
                    "created_at": child.created_at,
                    "terminal_at": child.terminal_at,
                })
            })
            .collect::<Vec<_>>();
        println!(
            "{}",
            serde_json::to_string_pretty(&payload).map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    if children.is_empty() {
        println!("no children");
        return Ok(());
    }
    for child in &children {
        println!(
            "{:<36} depth={} status={:<10} tokens={:>8} (in {:<8} out {:<8} reasoning {:<8}) {}",
            child.child_session_id,
            child.depth,
            child.status,
            child.total_tokens,
            child.input_tokens,
            child.output_tokens,
            child.reasoning_tokens,
            child.terminal_at.as_deref().unwrap_or(""),
        );
    }
    Ok(())
}
