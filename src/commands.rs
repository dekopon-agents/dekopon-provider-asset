use crate::{AssetProvider, Attach, Cat, List, Remove, Send, operations};
use clap::{Parser, Subcommand};
use dekopon_provider_sdk::provider::{Proposal, Usage};

#[derive(Parser)]
#[command(
    name = "asset",
    version,
    about = "Manage conversation assets; attaching does not send"
)]
pub struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// List conversation asset metadata.
    Ls,
    /// Remove an unsent asset.
    Rm { n: String },
    /// Queue an asset for this turn's reply.
    Send { n: String },
    /// Read bounded UTF-8 text from an asset.
    Cat { n: String },
    /// Attach piped text without sending it.
    Attach {
        #[arg(long = "type")]
        content_type: String,
    },
}

pub fn propose(args: Args, stdin_piped: bool) -> Result<Proposal<AssetProvider>, Usage> {
    match args.command {
        Command::Ls => Ok(Proposal::to::<List>(operations::Empty {})),
        Command::Rm { n } => Ok(Proposal::to::<Remove>(source(&n)?)),
        Command::Send { n } => Ok(Proposal::to::<Send>(source(&n)?)),
        Command::Cat { n } => Ok(Proposal::to::<Cat>(source(&n)?)),
        Command::Attach { content_type } => {
            operations::validate_type(&content_type).map_err(|e| Usage::new(e.to_string()))?;
            if !stdin_piped {
                return Err(Usage::new("asset attach requires piped UTF-8 text"));
            }
            Ok(Proposal::to::<Attach>(operations::Attachment {
                content_type,
                stdin_piped,
            }))
        }
    }
}

fn source(number: &str) -> Result<operations::Source, Usage> {
    let source = format!("chat-asset:{number}");
    operations::validate_reference(&source).map_err(|e| Usage::new(e.to_string()))?;
    Ok(operations::Source { source })
}
