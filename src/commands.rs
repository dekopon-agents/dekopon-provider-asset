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
    Rm {
        /// Reference as its own unquoted word, for example chat-asset:7.
        #[arg(value_name = "chat-asset:N")]
        reference: String,
    },
    /// Queue an asset for this turn's reply.
    Send {
        /// Reference as its own unquoted word, for example chat-asset:7.
        #[arg(value_name = "chat-asset:N")]
        reference: String,
    },
    /// Read bounded UTF-8 text from an asset.
    Cat {
        /// Reference as its own unquoted word, for example chat-asset:7.
        #[arg(value_name = "chat-asset:N")]
        reference: String,
    },
    /// Attach piped text without sending it.
    Attach {
        #[arg(long = "type")]
        content_type: String,
    },
}

pub fn propose(args: Args, stdin_piped: bool) -> Result<Proposal<AssetProvider>, Usage> {
    match args.command {
        Command::Ls => Ok(Proposal::to::<List>(operations::Empty {})),
        Command::Rm { reference } => Ok(Proposal::to::<Remove>(source(reference)?)),
        Command::Send { reference } => Ok(Proposal::to::<Send>(source(reference)?)),
        Command::Cat { reference } => Ok(Proposal::to::<Cat>(source(reference)?)),
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

fn source(source: String) -> Result<operations::Source, Usage> {
    operations::validate_reference(&source).map_err(|_| {
        Usage::new(
            "expected chat-asset:N as an unquoted word, with N a canonical unsigned 64-bit decimal number",
        )
    })?;
    Ok(operations::Source { source })
}
