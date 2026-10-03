use crate::{
    AssetProvider, MAX_TEXT_BYTES,
    operations::{self, AssetAccess, Attachment, Source},
};
use dekopon_provider_sdk::{
    CommandRunOutcome, EffectKind,
    asset::{AssetError, AssetErrorCode, Encoding, Info},
    provider::{self, Failure},
};
use serde_json::json;
use std::{
    cell::{Cell, RefCell},
    io::Cursor,
};

struct Fake {
    calls: RefCell<Vec<&'static str>>,
    data: Vec<u8>,
    cursor: Cell<usize>,
    kind: String,
    fail: Option<&'static str>,
}
impl Fake {
    fn new(kind: &str, data: &[u8]) -> Self {
        Self {
            calls: RefCell::new(Vec::new()),
            data: data.to_vec(),
            cursor: Cell::new(0),
            kind: kind.into(),
            fail: None,
        }
    }
    fn record(&self, name: &'static str) {
        self.calls.borrow_mut().push(name);
    }
    fn check(&self, name: &'static str) -> Result<(), AssetError> {
        self.record(name);
        if self.fail == Some(name) {
            return Err(AssetError {
                code: AssetErrorCode::Denied,
                message: "fake host denial".into(),
            });
        }
        Ok(())
    }
}
impl AssetAccess for Fake {
    type Handle = bool;
    type Writer = ();
    fn list(&self) -> Vec<Info> {
        self.record("list");
        vec![self.info(&false)]
    }
    fn open(&self, reference: &str) -> Result<bool, AssetError> {
        assert_eq!(reference, "chat-asset:7");
        self.check("open")?;
        Ok(false)
    }
    fn info(&self, attached: &bool) -> Info {
        Info {
            id: if *attached { None } else { Some(7) },
            content_type: self.kind.clone(),
            encoding: Encoding::Identity,
            stored_bytes: Some(self.data.len() as u64),
            seekable: true,
            origin: "chat".into(),
            sent: false,
        }
    }
    fn read(&self, _: &bool, buf: &mut [u8]) -> Result<usize, AssetError> {
        self.check("read")?;
        let start = self.cursor.get();
        let n = buf.len().min(997).min(self.data.len() - start);
        buf[..n].copy_from_slice(&self.data[start..start + n]);
        self.cursor.set(start + n);
        Ok(n)
    }
    fn remove(&self, _: &bool) -> Result<(), AssetError> {
        self.check("remove")
    }
    fn send(&self, _: &bool) -> Result<(), AssetError> {
        self.check("send")
    }
    fn allocate(&self, _: &str, _: Encoding) -> Result<(), AssetError> {
        self.check("allocate")
    }
    fn write_all(&self, _: &(), bytes: &[u8]) -> Result<(), AssetError> {
        assert_eq!(bytes, self.data);
        self.check("write")
    }
    fn attach(&self, _: ()) -> Result<bool, AssetError> {
        self.check("attach")?;
        Ok(true)
    }
}
fn source() -> Source {
    Source {
        source: "chat-asset:7".into(),
    }
}
fn words(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|p| (*p).into()).collect()
}
#[test]
fn typed_manifest_and_proposals_preserve_authority() {
    let m = provider::manifest::<AssetProvider>().unwrap();
    assert_eq!(m.id.as_str(), "asset");
    for (cap, name, effect) in m
        .capabilities
        .iter()
        .zip([
            "asset.ls",
            "asset.rm",
            "asset.send",
            "asset.cat",
            "asset.attach",
        ])
        .zip([
            EffectKind::ReadOnly,
            EffectKind::LocalWrite,
            EffectKind::ExternalWrite,
            EffectKind::ReadOnly,
            EffectKind::LocalWrite,
        ])
        .map(|((a, b), c)| (a, b, c))
    {
        assert_eq!(cap.id.as_str(), name);
        assert_eq!(cap.effect, effect);
        assert_eq!(cap.input_schema["additionalProperties"], false);
    }
    for (cmd, piped, cap, input) in [
        (vec!["ls"], false, "asset.ls", json!({})),
        (
            vec!["rm", "7"],
            false,
            "asset.rm",
            json!({"source":"chat-asset:7"}),
        ),
        (
            vec!["send", "7"],
            false,
            "asset.send",
            json!({"source":"chat-asset:7"}),
        ),
        (
            vec!["cat", "7"],
            false,
            "asset.cat",
            json!({"source":"chat-asset:7"}),
        ),
        (
            vec!["attach", "--type", "text/plain"],
            true,
            "asset.attach",
            json!({"content_type":"text/plain", "stdin_piped":true}),
        ),
    ] {
        let CommandRunOutcome::Proposed {
            capability,
            input: got,
            secret_use,
        } = provider::command::<AssetProvider>(&words(&cmd), piped)
        else {
            panic!("proposal for {cmd:?}")
        };
        assert_eq!(capability.as_str(), cap);
        assert_eq!(got, input);
        assert!(secret_use.is_none());
    }
    assert!(matches!(
        provider::command::<AssetProvider>(&words(&["attach", "--type", "text/plain"]), false),
        CommandRunOutcome::Failed { .. }
    ));
}
#[test]
fn attach_refuses_invalid_utf8_and_overlong_before_effects() {
    for data in [vec![0xff], vec![b'x'; MAX_TEXT_BYTES + 1]] {
        let fake = Fake::new("text/plain", &data);
        let mut output = Vec::new();
        assert!(
            operations::attach_from(
                &fake,
                Attachment {
                    content_type: "text/plain".into(),
                    stdin_piped: true
                },
                Cursor::new(data),
                &mut output
            )
            .is_err()
        );
        assert!(fake.calls.borrow().is_empty());
        assert!(output.is_empty());
    }
    for text in ["", "héllo"] {
        let fake = Fake::new("text/plain", text.as_bytes());
        let mut out = Vec::new();
        operations::attach_from(
            &fake,
            Attachment {
                content_type: "text/plain".into(),
                stdin_piped: true,
            },
            Cursor::new(text),
            &mut out,
        )
        .unwrap();
        assert_eq!(*fake.calls.borrow(), ["allocate", "write", "attach"]);
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&out).unwrap()["attached"]["id"],
            json!(null)
        );
    }
}
#[test]
fn send_remove_and_cat_keep_distinct_effects_and_bounded_text() {
    let fake = Fake::new("text/plain", b"hello");
    let mut out = Vec::new();
    operations::remove(&fake, source(), &mut out).unwrap();
    assert_eq!(*fake.calls.borrow(), ["open", "remove"]);
    fake.calls.borrow_mut().clear();
    out.clear();
    operations::send(&fake, source(), &mut out).unwrap();
    assert_eq!(*fake.calls.borrow(), ["open", "send"]);
    fake.calls.borrow_mut().clear();
    out.clear();
    operations::cat(&fake, source(), &mut out).unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&out).unwrap()["text"],
        "hello"
    );
    let bad = Fake::new("image/png", b"private");
    let mut out = Vec::new();
    assert!(operations::cat(&bad, source(), &mut out).is_err());
    assert!(out.is_empty());
    assert_eq!(*bad.calls.borrow(), ["open"]);
    let huge = Fake::new("text/plain", &vec![b'x'; MAX_TEXT_BYTES + 1]);
    assert!(operations::cat(&huge, source(), &mut Vec::new()).is_err());
    assert_eq!(huge.cursor.get(), MAX_TEXT_BYTES + 1);
}

#[test]
fn malformed_references_and_mime_are_refused_before_imports() {
    let fake = Fake::new("text/plain", b"hello");
    let mut output = Vec::new();
    for reference in [
        "data:text/plain,hi",
        "chat-asset:",
        "chat-asset:07",
        "chat-asset:+7",
        "chat-asset:7\n",
        "chat-asset:٧",
        "chat-asset:18446744073709551616",
    ] {
        for call in [
            operations::remove as fn(&Fake, Source, &mut Vec<u8>) -> _,
            operations::send,
            operations::cat,
        ] {
            let error = call(
                &fake,
                Source {
                    source: reference.into(),
                },
                &mut output,
            )
            .unwrap_err();
            assert_eq!(error.code().as_str(), "invalid-input");
        }
    }
    for value in ["chat-asset:0", "chat-asset:18446744073709551615"] {
        operations::validate_reference(value).unwrap();
    }
    for mime in ["image/*", "text/plain\r\nx:y", "", &"a".repeat(256)] {
        let error = operations::attach_from(
            &fake,
            Attachment {
                content_type: mime.into(),
                stdin_piped: true,
            },
            Cursor::new(b"hello"),
            &mut output,
        )
        .unwrap_err();
        assert_eq!(error.code().as_str(), "invalid-input");
        assert!(matches!(
            provider::command::<AssetProvider>(&words(&["attach", "--type", mime]), true),
            CommandRunOutcome::Failed { .. }
        ));
    }
    assert!(fake.calls.borrow().is_empty());
    assert!(output.is_empty());
}

#[test]
fn listing_is_metadata_only_and_cat_checks_utf8_and_mime() {
    let fake = Fake::new("image/png", b"private bytes");
    let mut out = Vec::new();
    operations::list(&fake, operations::Empty {}, &mut out).unwrap();
    let listed: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(listed["assets"][0]["id"], 7);
    assert_eq!(listed["assets"][0]["content_type"], "image/png");
    assert!(!String::from_utf8(out).unwrap().contains("private bytes"));
    assert_eq!(*fake.calls.borrow(), ["list"]);
    for kind in [
        "text/plain; charset=utf-8",
        "application/json",
        "application/problem+json",
        "application/xml",
        "application/atom+xml",
        "application/javascript",
    ] {
        let fake = Fake::new(kind, "🦀\n".repeat(1000).as_bytes());
        let mut out = Vec::new();
        operations::cat(&fake, source(), &mut out).unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&out).unwrap()["text"],
            "🦀\n".repeat(1000)
        );
    }
    let bad = Fake::new("text/plain", &[0xff]);
    let mut out = Vec::new();
    let error = operations::cat(&bad, source(), &mut out).unwrap_err();
    assert_eq!(error.code().as_str(), "invalid-text");
    assert!(out.is_empty());
}

#[test]
fn host_failures_preserve_code_and_stop_later_effects() {
    for (operation, failing, expected) in [
        ("rm", "open", &["open"][..]),
        ("rm", "remove", &["open", "remove"][..]),
        ("send", "send", &["open", "send"][..]),
        ("cat", "read", &["open", "read"][..]),
        ("attach", "allocate", &["allocate"][..]),
        ("attach", "write", &["allocate", "write"][..]),
        ("attach", "attach", &["allocate", "write", "attach"][..]),
    ] {
        let mut fake = Fake::new("text/plain", b"hi");
        fake.fail = Some(failing);
        let mut out = Vec::new();
        let error = match operation {
            "rm" => operations::remove(&fake, source(), &mut out),
            "send" => operations::send(&fake, source(), &mut out),
            "cat" => operations::cat(&fake, source(), &mut out),
            "attach" => operations::attach_from(
                &fake,
                Attachment {
                    content_type: "text/plain".into(),
                    stdin_piped: true,
                },
                Cursor::new(b"hi"),
                &mut out,
            ),
            _ => unreachable!(),
        }
        .unwrap_err();
        assert_eq!(error.code().as_str(), "denied");
        assert_eq!(error.to_string(), "fake host denial");
        assert_eq!(*fake.calls.borrow(), expected);
        assert!(out.is_empty());
    }
}
