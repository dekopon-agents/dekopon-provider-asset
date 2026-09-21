use crate::{ATTACH, CAT, LS, RM, SEND, operations};
use dekopon_provider_sdk::{
    CommandInvocation, CommandRun, ProviderError,
    clap::{Arg, Command},
    cli,
};
use serde_json::json;

pub(crate) fn run(argv: &[String], stdin: Option<&str>) -> Result<CommandRun, ProviderError> {
    let mut tree = Command::new("asset")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Manage conversation assets; attaching does not send")
        .subcommand_required(true)
        .subcommand(Command::new("ls").about("List conversation asset metadata"));
    for (name, about) in [
        ("rm", "Remove an unsent asset"),
        ("send", "Queue an asset for this turn's reply"),
        ("cat", "Read bounded UTF-8 text from an asset"),
    ] {
        tree = tree.subcommand(
            Command::new(name)
                .about(about)
                .arg(Arg::new("N").required(true)),
        );
    }
    tree = tree.subcommand(
        Command::new("attach")
            .about("Attach piped text without sending it")
            .arg(
                Arg::new("type")
                    .long("type")
                    .required(true)
                    .value_name("MIME"),
            ),
    );
    cli::run_command(tree, argv, stdin, |matches, stdin| {
        let (capability, input) = match matches.subcommand() {
            Some(("ls", _)) => (LS, json!({})),
            Some((name @ ("rm" | "send" | "cat"), args)) => {
                let number = args.get_one::<String>("N").expect("required number");
                // Store the whole reference as a string leaf: the gateway pins it before invoke.
                let reference = format!("chat-asset:{number}");
                operations::validate_reference(&reference)?;
                let capability = match name {
                    "rm" => RM,
                    "send" => SEND,
                    _ => CAT,
                };
                (capability, json!({"source": reference}))
            }
            Some(("attach", args)) => {
                let content_type = args.get_one::<String>("type").expect("required type");
                let text = stdin
                    .ok_or_else(|| crate::invalid("asset attach requires piped UTF-8 text"))?;
                operations::validate_attach(content_type, text)?;
                (ATTACH, json!({"content_type": content_type, "text": text}))
            }
            _ => unreachable!("clap requires one known subcommand"),
        };
        Ok(CommandInvocation {
            capability: capability.parse().expect("static capability"),
            input,
            secret_use: None,
        })
    })
}
