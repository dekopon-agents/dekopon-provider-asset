//! Real SDK/broker-host boundary. Asset transport itself is covered by native injected tests:
//! testkit 0.18.0 deliberately has no asset configuration or descriptor-input builder.
use dekopon_provider_sdk_testkit::{
    BrokerHostError, CommandRunOutcome, FakeBroker, FakeBrokerError,
};
use serde_json::json;
use std::path::PathBuf;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn component_proposes_all_commands_and_refuses_unauthorized_asset_access()
-> Result<(), Box<dyn std::error::Error>> {
    let component = PathBuf::from(
        std::env::var_os("DEKOPON_PROVIDER_COMPONENT")
            .expect("DEKOPON_PROVIDER_COMPONENT must point at the freshly built component"),
    );
    // One host in this suite, unique cold-cache directory across separate test processes/runs.
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_nanos();
    let cache = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join(format!("component-cache-{}-{nonce}", std::process::id()));
    std::fs::create_dir_all(&cache)?;
    let broker = FakeBroker::builder()
        .component(component)
        .provider("asset")
        .compile_cache(cache)
        .build()
        .await?;
    let words = |argv: &[&str]| argv.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
    let CommandRunOutcome::Rendered {
        stdout,
        stderr,
        status,
    } = broker
        .run_command("asset", &words(&["--help"]), None)
        .await?
    else {
        panic!("help renders")
    };
    assert_eq!(status, 0);
    assert!(stderr.is_empty());
    assert!(stdout.contains("attach"));
    for (argv, stdin, expected, input) in [
        (vec!["ls"], None, "asset.ls", json!({})),
        (
            vec!["rm", "7"],
            None,
            "asset.rm",
            json!({"source":"chat-asset:7"}),
        ),
        (
            vec!["send", "7"],
            None,
            "asset.send",
            json!({"source":"chat-asset:7"}),
        ),
        (
            vec!["cat", "7"],
            None,
            "asset.cat",
            json!({"source":"chat-asset:7"}),
        ),
        (
            vec!["attach", "--type", "text/plain"],
            Some("hello"),
            "asset.attach",
            json!({"content_type":"text/plain","text":"hello"}),
        ),
    ] {
        let CommandRunOutcome::Proposed {
            capability,
            input: actual,
            secret_use,
        } = broker.run_command("asset", &words(&argv), stdin).await?
        else {
            panic!("pure proposal")
        };
        assert_eq!(capability.as_str(), expected);
        assert_eq!(actual, input);
        assert!(secret_use.is_none());
    }
    assert_eq!(
        broker.invoke("asset.ls", json!({})).await?,
        json!({"assets":[]})
    );
    let invalid = broker
        .invoke("asset.cat", json!({"source":"data:text/plain,x"}))
        .await
        .expect_err("invalid source refused");
    assert_eq!(
        invalid.provider_failure().expect("guest refusal").0,
        "invalid-input"
    );
    for capability in ["asset.cat", "asset.rm", "asset.send"] {
        let failure = broker
            .invoke(capability, json!({"source":"chat-asset:7"}))
            .await
            .expect_err("no input descriptor");
        // The real host rejects a referenced proposal without its descriptor before invoke.
        let FakeBrokerError::Invocation(failure) = failure else {
            panic!("expected invocation failure")
        };
        let BrokerHostError::AssetInput { source } = failure.error.as_ref() else {
            panic!("expected asset admission failure: {failure:?}")
        };
        assert_eq!(
            source.to_string(),
            "asset descriptor count does not match references"
        );
    }
    let failure = broker
        .invoke(
            "asset.attach",
            json!({"content_type":"text/plain","text":"hello"}),
        )
        .await
        .expect_err("no asset grant/config");
    // Sticky host authority takes precedence over the guest's typed SDK error.
    let FakeBrokerError::Invocation(failure) = failure else {
        panic!("expected invocation failure")
    };
    assert!(
        matches!(
            failure.error.as_ref(),
            BrokerHostError::HostCallRejected {
                reason: "asset-call-rejected",
                ..
            }
        ),
        "{failure:?}"
    );
    Ok(())
}
