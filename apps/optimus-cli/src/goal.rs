//! `optimus goal` — the CLI surface for the session goal (spec-026 R7).

use clap::{Args, Subcommand};
use optimus_kernel::Kernel;

#[derive(Subcommand, Debug)]
pub enum GoalCmd {
    /// Create or rewrite the session goal in `idle` (needs objective)
    Set {
        /// Non-empty objective
        #[arg(long)]
        objective: String,
        /// Token budget (positive; enforced by the turn loop while active)
        #[arg(long)]
        token_budget: Option<u64>,
        /// Time budget in seconds (positive; enforced while active)
        #[arg(long)]
        time_budget_seconds: Option<u64>,
    },
    /// Start the goal: idle to active
    Start,
    /// Print the goal record
    Status,
    /// Pause the goal: active to paused (freezes accounting)
    Pause,
    /// Resume the goal: paused to active
    Resume,
    /// Complete the goal: active/paused to complete
    Complete,
}

#[derive(Args, Debug)]
pub struct GoalArgs {
    #[command(subcommand)]
    pub cmd: GoalCmd,
    /// Resume session uuid (default: a fresh session)
    #[arg(long, global = true)]
    pub session: Option<String>,
    /// Emit machine-readable JSON
    #[arg(long, global = true)]
    pub json: bool,
}

pub fn run_goal(kernel: &mut Kernel, args: &GoalArgs) -> Result<(), Box<dyn std::error::Error>> {
    match &args.cmd {
        GoalCmd::Set {
            objective,
            token_budget,
            time_budget_seconds,
        } => {
            let goal = kernel.goal_set(objective.clone(), *token_budget, *time_budget_seconds)?;
            print_goal(kernel, &goal, args.json)?;
        }
        GoalCmd::Start => {
            let goal = kernel.goal_start()?;
            print_goal(kernel, &goal, args.json)?
        }
        GoalCmd::Pause => {
            let goal = kernel.goal_pause()?;
            print_goal(kernel, &goal, args.json)?
        }
        GoalCmd::Resume => {
            let goal = kernel.goal_resume()?;
            print_goal(kernel, &goal, args.json)?
        }
        GoalCmd::Complete => {
            let goal = kernel.goal_complete()?;
            print_goal(kernel, &goal, args.json)?
        }
        GoalCmd::Status => match kernel.goal()? {
            Some(goal) => print_goal(kernel, &goal, args.json)?,
            None => {
                if args.json {
                    println!("{{\"goal\": null}}");
                } else {
                    println!(
                        "no goal set for session {} (use `optimus goal set --objective \"...\"`)",
                        kernel.session_id()
                    );
                }
            }
        },
    }
    Ok(())
}

fn print_goal(
    kernel: &Kernel,
    goal: &optimus_kernel::Goal,
    json: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    if json {
        println!("{}", serde_json::to_string_pretty(goal)?);
    } else {
        println!("goal   {}", goal.objective);
        println!("id     {}", goal.id);
        println!("status {}", goal.status.as_str());
        println!(
            "budget token={} time_s={}",
            goal.token_budget
                .map(|n| n.to_string())
                .unwrap_or_else(|| "-".into()),
            goal.time_budget_seconds
                .map(|n| n.to_string())
                .unwrap_or_else(|| "-".into())
        );
        println!(
            "used   tokens={} active_s={}",
            goal.tokens_used, goal.active_seconds
        );
        if let Some(error) = &goal.error {
            println!("error  {error}");
        }
        println!("session {}", kernel.session_id());
    }
    Ok(())
}
