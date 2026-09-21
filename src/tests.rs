use crate::{
    ATTACH, AssetProvider, CAT, LS, MAX_ENVELOPE_BYTES, MAX_TEXT_BYTES, RM, SEND,
    operations::{self, Assets},
};
use dekopon_provider_sdk::{
    CommandRun, EffectKind, Provider,
    asset::{AssetError, AssetErrorCode, Encoding, Info},
};
use serde_json::json;
use std::cell::{Cell, RefCell};

struct Fake {
    calls: RefCell<Vec<String>>,
    data: Vec<u8>,
    cursor: Cell<usize>,
    content_type: String,
    fail: Option<&'static str>,
    stored: Option<u64>,
    encoding: Encoding,
}
impl Fake {
    fn new(content_type: &str, data: &[u8]) -> Self {
        Self {
            calls: RefCell::default(),
            data: data.to_vec(),
            cursor: Cell::new(0),
            content_type: content_type.into(),
            fail: None,
            stored: Some(data.len() as u64),
            encoding: Encoding::Identity,
        }
    }
    fn call(&self, operation: &str) -> Result<(), AssetError> {
        self.calls.borrow_mut().push(operation.into());
        if self.fail == Some(operation) {
            return Err(AssetError {
                code: AssetErrorCode::Denied,
                message: "fake host denial".into(),
            });
        }
        Ok(())
    }
}
impl Assets for Fake {
    type Handle = bool;
    type Writer = ();
    fn list(&self) -> Vec<Info> {
        self.call("list").unwrap();
        vec![self.info(&false)]
    }
    fn open(&self, reference: &str) -> Result<bool, AssetError> {
        assert_eq!(reference, "chat-asset:7");
        self.call("open")?;
        Ok(false)
    }
    fn info(&self, attached: &bool) -> Info {
        Info {
            id: if *attached { None } else { Some(7) },
            content_type: self.content_type.clone(),
            encoding: self.encoding,
            stored_bytes: self.stored,
            seekable: true,
            origin: "chat".into(),
            sent: false,
        }
    }
    fn read(&self, _: &bool, buffer: &mut [u8]) -> Result<usize, AssetError> {
        self.call("read")?;
        // Deliberate short reads prove the caller loops, including across UTF-8 boundaries.
        let start = self.cursor.get();
        let count = buffer.len().min(997).min(self.data.len() - start);
        buffer[..count].copy_from_slice(&self.data[start..start + count]);
        self.cursor.set(start + count);
        Ok(count)
    }
    fn remove(&self, _: &bool) -> Result<(), AssetError> {
        self.call("remove")
    }
    fn send(&self, _: &bool) -> Result<(), AssetError> {
        self.call("send")
    }
    fn allocate(&self, content_type: &str, encoding: Encoding) -> Result<(), AssetError> {
        assert_eq!(content_type, self.content_type);
        assert!(matches!(encoding, Encoding::Identity));
        self.call("allocate")
    }
    fn write_all(&self, _: &(), bytes: &[u8]) -> Result<(), AssetError> {
        assert_eq!(bytes, self.data);
        self.call("write")
    }
    fn attach(&self, _: ()) -> Result<bool, AssetError> {
        self.call("attach")?;
        Ok(true)
    }
}
fn invoke(
    fake: &Fake,
    cap: &str,
) -> Result<serde_json::Value, dekopon_provider_sdk::ProviderError> {
    operations::invoke(fake, cap, json!({"source":"chat-asset:7"}))
}
fn argv(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| (*w).into()).collect()
}

#[test]
fn mirrors_and_manifest_match_the_contract() {
    assert_eq!(
        include_str!("../wit/deps/provider.wit"),
        dekopon_provider_sdk::PROVIDER_WIT
    );
    assert_eq!(
        include_str!("../wit/deps/asset.wit"),
        dekopon_provider_sdk::ASSET_WIT
    );
    let m = AssetProvider::manifest();
    assert_eq!(m.id.as_str(), "asset");
    assert_eq!(m.command_words, ["asset"]);
    let expected = [
        (LS, EffectKind::ReadOnly),
        (RM, EffectKind::LocalWrite),
        (SEND, EffectKind::ExternalWrite),
        (CAT, EffectKind::ReadOnly),
        (ATTACH, EffectKind::LocalWrite),
    ];
    assert_eq!(m.capabilities.len(), expected.len());
    for (c, (id, effect)) in m.capabilities.iter().zip(expected) {
        assert_eq!(c.id.as_str(), id);
        assert_eq!(c.effect, effect);
        assert_eq!(c.input_schema["additionalProperties"], false);
    }
}
#[test]
fn commands_are_pure_and_reference_leaves_are_exact() {
    for (words, stdin, capability, input) in [
        (vec!["ls"], None, LS, json!({})),
        (vec!["rm", "7"], None, RM, json!({"source":"chat-asset:7"})),
        (
            vec!["send", "7"],
            None,
            SEND,
            json!({"source":"chat-asset:7"}),
        ),
        (
            vec!["cat", "7"],
            None,
            CAT,
            json!({"source":"chat-asset:7"}),
        ),
        (
            vec!["attach", "--type", "text/plain"],
            Some("hello"),
            ATTACH,
            json!({"content_type":"text/plain","text":"hello"}),
        ),
    ] {
        let CommandRun::Proposal(p) = AssetProvider::run_command(&argv(&words), stdin).unwrap()
        else {
            panic!("expected proposal")
        };
        assert_eq!(p.capability.as_str(), capability);
        assert_eq!(p.input, input);
        assert!(p.secret_use.is_none());
    }
    for words in [vec!["--help"], vec!["--version"]] {
        assert!(matches!(
            AssetProvider::run_command(&argv(&words), None).unwrap(),
            CommandRun::Rendered { status: 0, .. }
        ));
    }
    for words in [vec!["bogus"], vec!["cat"], vec!["attach"]] {
        assert!(matches!(
            AssetProvider::run_command(&argv(&words), None).unwrap(),
            CommandRun::Rendered { status: 2, .. }
        ));
    }
    assert!(AssetProvider::run_command(&argv(&["attach", "--type", "text/plain"]), None).is_err());
}
#[test]
fn invalid_inputs_never_reach_imports() {
    let fake = Fake::new("text/plain", b"");
    for (cap, input) in [
        (LS, json!({"extra":true})),
        (CAT, json!({"source":"data:text/plain,hi"})),
        (RM, json!({"source":"chat-asset:07"})),
        (SEND, json!({"source":7})),
        (CAT, json!({"source":"chat-asset:7","extra":0})),
        (CAT, json!({"source":"chat-asset:18446744073709551616"})),
        (ATTACH, json!({"content_type":"image/*","text":""})),
        (
            ATTACH,
            json!({"content_type":"text/plain\r\nx:y","text":""}),
        ),
        (
            ATTACH,
            json!({"content_type":"text/plain","text":"x".repeat(MAX_TEXT_BYTES+1)}),
        ),
        ("asset.unknown", json!({})),
    ] {
        assert!(operations::invoke(&fake, cap, input).is_err());
    }
    assert!(fake.calls.borrow().is_empty());
    for value in ["chat-asset:0", "chat-asset:18446744073709551615"] {
        operations::validate_reference(value).unwrap();
    }
    for value in [
        "chat-asset:",
        "chat-asset:+7",
        "chat-asset:7\n",
        "chat-asset:٧",
    ] {
        assert!(operations::validate_reference(value).is_err());
    }
}
#[test]
fn list_is_metadata_only_remove_and_send_are_distinct() {
    let fake = Fake::new("image/png", b"secret bytes");
    let listed = operations::invoke(&fake, LS, json!({})).unwrap();
    assert_eq!(listed["assets"][0]["id"], 7);
    assert_eq!(fake.calls.borrow().as_slice(), ["list"]);
    assert!(!listed.to_string().contains("secret bytes"));
    fake.calls.borrow_mut().clear();
    assert_eq!(
        invoke(&fake, RM).unwrap(),
        json!({"removed":"chat-asset:7"})
    );
    assert_eq!(fake.calls.borrow().as_slice(), ["open", "remove"]);
    fake.calls.borrow_mut().clear();
    assert_eq!(
        invoke(&fake, SEND).unwrap(),
        json!({"queued":"chat-asset:7"})
    );
    assert_eq!(fake.calls.borrow().as_slice(), ["open", "send"]);
}
#[test]
fn attach_writes_identity_and_returns_only_unnumbered_metadata() {
    for data in ["", "héllo"] {
        let fake = Fake::new("text/plain", data.as_bytes());
        let output = operations::invoke(
            &fake,
            ATTACH,
            json!({"content_type":"text/plain","text":data}),
        )
        .unwrap();
        assert_eq!(
            fake.calls.borrow().as_slice(),
            ["allocate", "write", "attach"]
        );
        assert!(output["attached"]["id"].is_null());
        assert_eq!(output["attached"]["bytes"], data.len());
        assert_eq!(output["attached"]["encoding"], "identity");
        assert!(output.get("attachments").is_none());
    }
}
#[test]
fn cat_reads_only_text_and_enforces_decoded_not_advertised_size() {
    for content_type in [
        "text/plain; charset=utf-8",
        "application/json",
        "application/problem+json",
        "application/xml",
        "application/atom+xml",
        "application/javascript",
    ] {
        let mut fake = Fake::new(content_type, "🦀\n".repeat(1000).as_bytes());
        fake.stored = Some(u64::MAX);
        fake.encoding = Encoding::Base64;
        assert_eq!(invoke(&fake, CAT).unwrap()["text"], "🦀\n".repeat(1000));
    }
    let binary = Fake::new("image/png", b"not read");
    let error = invoke(&binary, CAT).unwrap_err();
    assert_eq!(error.code(), "binary-content");
    assert!(error.message().contains("image/png"));
    assert_eq!(binary.calls.borrow().as_slice(), ["open"]);
    let bad_utf8 = Fake::new("text/plain", &[0xff]);
    assert_eq!(invoke(&bad_utf8, CAT).unwrap_err().code(), "invalid-text");
    let mut largest = Fake::new("text/plain", &vec![0; MAX_TEXT_BYTES]);
    largest.stored = None;
    let output = invoke(&largest, CAT).unwrap();
    let envelope = dekopon_provider_sdk::ComponentResponse::Succeeded { output };
    assert!(serde_json::to_vec(&envelope).unwrap().len() < MAX_ENVELOPE_BYTES);
    let oversized = Fake::new("text/plain", &vec![0; MAX_TEXT_BYTES + 4096]);
    assert_eq!(invoke(&oversized, CAT).unwrap_err().code(), "too-large");
    assert_eq!(oversized.cursor.get(), MAX_TEXT_BYTES + 1);
}
#[test]
fn host_failures_stop_without_retry_or_later_effects() {
    for (cap, failing, expected) in [
        (RM, "open", vec!["open"]),
        (RM, "remove", vec!["open", "remove"]),
        (SEND, "send", vec!["open", "send"]),
        (CAT, "read", vec!["open", "read"]),
        (ATTACH, "allocate", vec!["allocate"]),
        (ATTACH, "write", vec!["allocate", "write"]),
        (ATTACH, "attach", vec!["allocate", "write", "attach"]),
    ] {
        let mut fake = Fake::new("text/plain", b"hi");
        fake.fail = Some(failing);
        let result = if cap == ATTACH {
            operations::invoke(&fake, cap, json!({"content_type":"text/plain","text":"hi"}))
        } else {
            invoke(&fake, cap)
        };
        let err = result.unwrap_err();
        assert_eq!(err.code(), "denied");
        assert_eq!(err.message(), "fake host denial");
        assert_eq!(*fake.calls.borrow(), expected);
    }
}
