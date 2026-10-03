use dekopon_asset_provider::AssetProvider;
use dekopon_provider_sdk_testkit::{Harness, conformance};
use serde_json::json;
use std::path::PathBuf;

fn component() -> PathBuf {
    std::env::var_os("DEKOPON_PROVIDER_COMPONENT")
        .expect("DEKOPON_PROVIDER_COMPONENT must point at the freshly built component")
        .into()
}
#[test]
fn real_component_conforms_and_refuses_ungranted_asset_effects()
-> Result<(), Box<dyn std::error::Error>> {
    let path = component();
    conformance::<AssetProvider>(&path)?;
    let rejected = Harness::<AssetProvider>::get(&path)
        .stdin(b"hello".to_vec())
        .call(
            "asset.attach",
            json!({"content_type":"text/plain", "stdin_piped":true}),
        );
    assert!(
        rejected.is_err(),
        "asset authority cannot be supplied by a proposal alone"
    );
    Ok(())
}
