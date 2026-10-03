mod commands;
mod operations;

use dekopon_provider_sdk::provider::{Assets, Capability, Proposal, Provider, Stdout, Usage};
use dekopon_provider_sdk::{EffectKind, RiskLevel};

pub(crate) const MAX_TEXT_BYTES: usize = 128 * 1024;
pub(crate) const MAX_ENVELOPE_BYTES: usize = 1_000_000;

pub struct AssetProvider;
pub struct List;
pub struct Remove;
pub struct Send;
pub struct Cat;
pub struct Attach;

impl Provider for AssetProvider {
    const ID: &'static str = "asset";
    const COMMAND_WORDS: &'static [&'static str] = &["asset"];
    const DESCRIPTION: &'static str =
        "Manage conversation assets through broker-owned handles; attach is not send";
    type Args = commands::Args;
    type Capabilities = (List, Remove, Send, Cat, Attach);

    fn propose(args: Self::Args, stdin_piped: bool) -> Result<Proposal<Self>, Usage> {
        commands::propose(args, stdin_piped)
    }
}

macro_rules! capability {
    ($name:ident, $suffix:literal, $description:literal, $effect:expr, $risk:expr, $input:ty, $method:ident) => {
        impl Capability for $name {
            type Provider = AssetProvider;
            const NAME: &'static str = $suffix;
            const DESCRIPTION: &'static str = $description;
            const EFFECT: EffectKind = $effect;
            const RISK: RiskLevel = $risk;
            type Input = $input;
            type Needs = Assets;
            type Error = operations::AssetFailure;
            fn run(
                input: Self::Input,
                assets: Assets,
                out: &mut Stdout,
            ) -> Result<(), Self::Error> {
                operations::$method(&operations::Host(assets), input, out)
            }
        }
    };
}
capability!(
    List,
    "ls",
    "List metadata, not permission to open assets",
    EffectKind::ReadOnly,
    RiskLevel::Low,
    operations::Empty,
    list
);
capability!(
    Remove,
    "rm",
    "Remove an unsent asset (requires asset.remove)",
    EffectKind::LocalWrite,
    RiskLevel::Low,
    operations::Source,
    remove
);
capability!(
    Send,
    "send",
    "Queue delivery on this turn's reply (requires asset.send)",
    EffectKind::ExternalWrite,
    RiskLevel::Medium,
    operations::Source,
    send
);
capability!(
    Cat,
    "cat",
    "Read at most 131072 decoded bytes of UTF-8 text; binary refused",
    EffectKind::ReadOnly,
    RiskLevel::Low,
    operations::Source,
    cat
);
capability!(
    Attach,
    "attach",
    "Attach stdin text without sending it (requires asset.attach)",
    EffectKind::LocalWrite,
    RiskLevel::Low,
    operations::Attachment,
    attach
);

#[allow(unsafe_code)]
mod export {
    dekopon_provider_sdk::export!(super::AssetProvider);
}

#[cfg(test)]
mod tests;
