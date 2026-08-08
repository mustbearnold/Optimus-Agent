//! `optimus skills` — the skills registry CLI surface.

use std::path::Path;

use clap::Subcommand;
use optimus_skills::{SkillDraft, SkillRegistry};

#[derive(Subcommand, Debug)]
pub enum SkillsCmd {
    /// List non-deprecated skills
    List {
        #[arg(long)]
        all: bool,
    },
    /// Create a candidate skill
    Create {
        name: String,
        /// Skill body text
        body: String,
        /// Comma-separated permissions: fs,terminal,net,browser,memory_write
        #[arg(long, default_value = "fs")]
        perms: String,
        #[arg(long)]
        pin: bool,
    },
    /// Resolve best skill by name
    Resolve { name: String },
}

pub fn run_skills(skills_db: &Path, cmd: SkillsCmd) -> Result<(), Box<dyn std::error::Error>> {
    let reg = SkillRegistry::open(skills_db)?;
    match cmd {
        SkillsCmd::List { all } => {
            for s in reg.list(all)? {
                println!(
                    "{} v{} {:?} uses={} rate={:.2} perms={:?}",
                    s.name, s.version, s.status, s.uses, s.success_rate, s.permissions
                );
            }
            Ok(())
        }
        SkillsCmd::Create {
            name,
            body,
            perms,
            pin,
        } => {
            let permissions = crate::parsers::parse_perms(&perms)?;
            let id = reg.create(SkillDraft {
                name,
                body,
                permissions,
                pin,
            })?;
            let s = reg.get(id)?;
            println!("created {} v{} {:?} id={id}", s.name, s.version, s.status);
            Ok(())
        }
        SkillsCmd::Resolve { name } => {
            match reg.resolve(&name)? {
                Some(s) => {
                    println!(
                        "{} v{} {:?} id={} rate={:.2}",
                        s.name, s.version, s.status, s.id, s.success_rate
                    );
                    println!("{}", s.body);
                }
                None => println!("no skill named {name}"),
            }
            Ok(())
        }
    }
}
