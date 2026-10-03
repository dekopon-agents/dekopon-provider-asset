use dekopon_asset_provider::AssetProvider;
use dekopon_broker_host::BrokerHostError;
use dekopon_provider_sdk_testkit::{Harness, HarnessError, conformance};
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
    let HarnessError::Invocation(failure) = rejected.expect_err("no asset grant") else {
        panic!("expected invocation failure from broker host");
    };
    assert!(
        matches!(
            failure.error.as_ref(),
            BrokerHostError::HostCallRejected {
                reason: "asset-call-rejected",
                ..
            }
        ),
        "unexpected refusal: {failure:?}"
    );
    Ok(())
}
