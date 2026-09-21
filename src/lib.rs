//! Conversation asset commands. Proposals are pure; only invoke reaches host imports.
mod commands;
mod manifest;
mod operations;

use dekopon_provider_sdk::{CapabilityId, CommandRun, Provider, ProviderError, ProviderManifest};
use serde_json::Value;

pub(crate) const LS: &str = "asset.ls";
pub(crate) const RM: &str = "asset.rm";
pub(crate) const SEND: &str = "asset.send";
pub(crate) const CAT: &str = "asset.cat";
pub(crate) const ATTACH: &str = "asset.attach";
/// Bound decoded text before JSON escaping (worst case six serialized bytes per input byte).
pub(crate) const MAX_TEXT_BYTES: usize = 128 * 1024;
/// Leave headroom below the SDK host's default 1 MiB response bound.
pub(crate) const MAX_ENVELOPE_BYTES: usize = 1_000_000;

#[allow(unsafe_code)]
mod bindings {
    wit_bindgen::generate!({ path: "wit", world: "provider", generate_all });
}

#[allow(unsafe_code)]
mod export {
    use super::bindings;
    dekopon_provider_sdk::export_provider_with_cli!(super::AssetProvider, bindings);
}

/// Public for native tests of the same boundary exported by the component.
pub struct AssetProvider;

impl Provider for AssetProvider {
    fn manifest() -> ProviderManifest {
        manifest::manifest()
    }

    fn invoke(capability: &CapabilityId, input: Value) -> Result<Value, ProviderError> {
        operations::invoke(&operations::Host, capability.as_str(), input)
    }

    fn run_command(argv: &[String], stdin: Option<&str>) -> Result<CommandRun, ProviderError> {
        commands::run(argv, stdin)
    }
}

pub(crate) fn invalid(message: &str) -> ProviderError {
    ProviderError::new("invalid-input", message)
}

#[cfg(test)]
mod tests;
