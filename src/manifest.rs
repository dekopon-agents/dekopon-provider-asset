use crate::{ATTACH, CAT, LS, MAX_TEXT_BYTES, RM, SEND};
use dekopon_provider_sdk::{
    EffectKind, ProviderApiVersion, ProviderCapability, ProviderManifest, RiskLevel,
};
use serde_json::{Value, json};

pub(crate) fn manifest() -> ProviderManifest {
    let reference = json!({"type":"object", "properties":{"source":{"type":"string", "pattern":"^chat-asset:(0|[1-9][0-9]*)$", "description":"Exact reference; N must fit u64"}}, "required":["source"], "additionalProperties":false});
    let capability =
        |id: &str, description: &str, effect, risk, input_schema: Value| ProviderCapability {
            id: id.parse().expect("static capability"),
            description: description.into(),
            effect,
            risk,
            input_schema,
        };
    ProviderManifest {
        api_version: ProviderApiVersion::V1Alpha1,
        id: "asset".parse().expect("static provider"),
        description: "Manage conversation assets through broker-owned handles; attach is not send"
            .into(),
        command_words: vec!["asset".into()],
        capabilities: vec![
            capability(
                LS,
                "List metadata, not permission to open assets",
                EffectKind::ReadOnly,
                RiskLevel::Low,
                json!({"type":"object", "properties":{}, "additionalProperties":false}),
            ),
            capability(
                RM,
                "Remove an unsent asset (requires asset.remove)",
                EffectKind::LocalWrite,
                RiskLevel::Low,
                reference.clone(),
            ),
            capability(
                SEND,
                "Queue delivery on this turn's reply (requires asset.send)",
                EffectKind::ExternalWrite,
                RiskLevel::Medium,
                reference.clone(),
            ),
            capability(
                CAT,
                "Read at most 131072 decoded bytes of UTF-8 text; binary refused",
                EffectKind::ReadOnly,
                RiskLevel::Low,
                reference,
            ),
            capability(
                ATTACH,
                "Attach stdin text without sending it (requires asset.attach)",
                EffectKind::LocalWrite,
                RiskLevel::Low,
                json!({
                    "type":"object", "properties":{
                        "content_type":{"type":"string", "maxLength":255, "description":"Concrete MIME type; no wildcard or control bytes"},
                        "text":{"type":"string", "maxLength":MAX_TEXT_BYTES, "description":"At most 131072 UTF-8 bytes, enforced independently of Unicode character count"}
                    }, "required":["content_type","text"], "additionalProperties":false
                }),
            ),
        ],
    }
}
