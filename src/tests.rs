use crate::{
    AssetProvider, MAX_TEXT_BYTES,
    operations::{self, AssetAccess, Attachment, Source},
};
use dekopon_provider_sdk::{
    CommandRunOutcome, EffectKind,
    asset::{AssetError, Encoding, Info},
    provider,
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
}
impl Fake {
    fn new(kind: &str, data: &[u8]) -> Self {
        Self {
            calls: RefCell::new(Vec::new()),
            data: data.to_vec(),
            cursor: Cell::new(0),
            kind: kind.into(),
        }
    }
    fn record(&self, name: &'static str) {
        self.calls.borrow_mut().push(name);
    }
}
impl AssetAccess for Fake {
    type Handle = bool;
    type Writer = ();
    fn list(&self) -> Vec<Info> {
        self.record("list");
        vec![self.info(&false)]
    }
    fn open(&self, _: &str) -> Result<bool, AssetError> {
        self.record("open");
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
        self.record("read");
        let start = self.cursor.get();
        let n = buf.len().min(997).min(self.data.len() - start);
        buf[..n].copy_from_slice(&self.data[start..start + n]);
        self.cursor.set(start + n);
        Ok(n)
    }
    fn remove(&self, _: &bool) -> Result<(), AssetError> {
        self.record("remove");
        Ok(())
    }
    fn send(&self, _: &bool) -> Result<(), AssetError> {
        self.record("send");
        Ok(())
    }
    fn allocate(&self, _: &str, _: Encoding) -> Result<(), AssetError> {
        self.record("allocate");
        Ok(())
    }
    fn write_all(&self, _: &(), bytes: &[u8]) -> Result<(), AssetError> {
        assert_eq!(bytes, self.data);
        self.record("write");
        Ok(())
    }
    fn attach(&self, _: ()) -> Result<bool, AssetError> {
        self.record("attach");
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
