use crate::MAX_TEXT_BYTES;
use dekopon_provider_sdk::{
    asset::{self, AssetError, Encoding, Info},
    provider::{self, Assets as GrantedAssets, Code, Failure},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fmt,
    io::{Read, Write},
};

#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Empty {}
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Source {
    #[schemars(regex(pattern = "^chat-asset:(0|[1-9][0-9]*)$"))]
    pub source: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Attachment {
    #[schemars(length(max = 255))]
    pub content_type: String,
    /// Piped input is read only after the broker authorizes this proposal.
    pub stdin_piped: bool,
}

#[derive(Debug)]
pub struct AssetFailure {
    code: Code,
    message: String,
}
impl AssetFailure {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code: Code::new(code),
            message: message.into(),
        }
    }
}
impl fmt::Display for AssetFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl Failure for AssetFailure {
    fn code(&self) -> Code {
        self.code
    }
}
fn invalid(message: &'static str) -> AssetFailure {
    AssetFailure::new("invalid-input", message)
}
fn host_error(error: AssetError) -> AssetFailure {
    AssetFailure::new(error.code.as_str(), error.message)
}

pub fn validate_reference(reference: &str) -> Result<(), AssetFailure> {
    let number = reference
        .strip_prefix("chat-asset:")
        .ok_or_else(|| invalid("source must be chat-asset:<N>"))?;
    if number.is_empty()
        || !number.bytes().all(|b| b.is_ascii_digit())
        || number.parse::<u64>().is_err()
        || (number.len() > 1 && number.starts_with('0'))
    {
        return Err(invalid(
            "N must be a canonical unsigned 64-bit decimal number",
        ));
    }
    Ok(())
}
fn media_type(value: &str) -> Result<mime::Mime, AssetFailure> {
    if value.len() > 255 || !value.is_ascii() || value.bytes().any(|b| b.is_ascii_control()) {
        return Err(invalid(
            "content type must be a concrete MIME type of at most 255 ASCII bytes",
        ));
    }
    let parsed: mime::Mime = value
        .parse()
        .map_err(|_| invalid("invalid MIME content type"))?;
    if parsed.type_() == mime::STAR || parsed.subtype() == mime::STAR || value.contains('*') {
        return Err(invalid("wildcard content types are not allowed"));
    }
    Ok(parsed)
}
pub fn validate_type(value: &str) -> Result<(), AssetFailure> {
    media_type(value).map(|_| ())
}

pub trait AssetAccess {
    type Handle;
    type Writer;
    fn list(&self) -> Vec<Info>;
    fn open(&self, reference: &str) -> Result<Self::Handle, AssetError>;
    fn info(&self, handle: &Self::Handle) -> Info;
    fn read(&self, handle: &Self::Handle, buffer: &mut [u8]) -> Result<usize, AssetError>;
    fn remove(&self, handle: &Self::Handle) -> Result<(), AssetError>;
    fn send(&self, handle: &Self::Handle) -> Result<(), AssetError>;
    fn allocate(&self, content_type: &str, encoding: Encoding) -> Result<Self::Writer, AssetError>;
    fn write_all(&self, writer: &Self::Writer, bytes: &[u8]) -> Result<(), AssetError>;
    fn attach(&self, writer: Self::Writer) -> Result<Self::Handle, AssetError>;
}
pub struct Host(pub GrantedAssets);
impl AssetAccess for Host {
    type Handle = asset::Handle;
    type Writer = asset::Writer;
    fn list(&self) -> Vec<Info> {
        self.0.list()
    }
    fn open(&self, r: &str) -> Result<Self::Handle, AssetError> {
        self.0.open(r)
    }
    fn info(&self, h: &Self::Handle) -> Info {
        h.info()
    }
    fn read(&self, h: &Self::Handle, b: &mut [u8]) -> Result<usize, AssetError> {
        h.read(b)
    }
    fn remove(&self, h: &Self::Handle) -> Result<(), AssetError> {
        self.0.remove(h)
    }
    fn send(&self, h: &Self::Handle) -> Result<(), AssetError> {
        self.0.send(h)
    }
    fn allocate(&self, t: &str, e: Encoding) -> Result<Self::Writer, AssetError> {
        self.0.allocate(t, e)
    }
    fn write_all(&self, w: &Self::Writer, b: &[u8]) -> Result<(), AssetError> {
        w.write_all(b)
    }
    fn attach(&self, w: Self::Writer) -> Result<Self::Handle, AssetError> {
        self.0.attach(w)
    }
}
fn info_json(info: Info) -> Value {
    json!({"id": info.id, "content_type": info.content_type, "encoding": match info.encoding { Encoding::Identity => "identity", Encoding::Base64 => "base64" }, "bytes": info.stored_bytes, "seekable": info.seekable, "origin": info.origin, "sent": info.sent})
}
fn emit(out: &mut impl Write, value: Value) -> Result<(), AssetFailure> {
    let bytes = serde_json::to_vec(&value)
        .map_err(|_| AssetFailure::new("internal", "output serialization failed"))?;
    if bytes.len() + 1 > crate::MAX_ENVELOPE_BYTES {
        return Err(AssetFailure::new(
            "too-large",
            "serialized output exceeds provider bound",
        ));
    }
    out.write_all(&bytes)
        .and_then(|_| out.write_all(b"\n"))
        .map_err(|_| AssetFailure::new("output-closed", "stdout's reader has gone"))
}
pub fn list(a: &impl AssetAccess, _: Empty, out: &mut impl Write) -> Result<(), AssetFailure> {
    emit(
        out,
        json!({"assets": a.list().into_iter().map(info_json).collect::<Vec<_>>()}),
    )
}
pub fn remove(
    a: &impl AssetAccess,
    input: Source,
    out: &mut impl Write,
) -> Result<(), AssetFailure> {
    validate_reference(&input.source)?;
    let h = a.open(&input.source).map_err(host_error)?;
    a.remove(&h).map_err(host_error)?;
    emit(out, json!({"removed": input.source}))
}
pub fn send(a: &impl AssetAccess, input: Source, out: &mut impl Write) -> Result<(), AssetFailure> {
    validate_reference(&input.source)?;
    let h = a.open(&input.source).map_err(host_error)?;
    a.send(&h).map_err(host_error)?;
    emit(out, json!({"queued": input.source}))
}
pub fn cat(a: &impl AssetAccess, input: Source, out: &mut impl Write) -> Result<(), AssetFailure> {
    validate_reference(&input.source)?;
    let h = a.open(&input.source).map_err(host_error)?;
    let info = a.info(&h);
    let parsed = media_type(&info.content_type)?;
    let text_type = parsed.type_() == mime::TEXT
        || (parsed.type_() == mime::APPLICATION
            && (matches!(
                parsed.subtype().as_str(),
                "json" | "xml" | "javascript" | "x-www-form-urlencoded"
            ) || parsed
                .suffix()
                .is_some_and(|s| matches!(s.as_str(), "json" | "xml"))));
    if !text_type {
        return Err(AssetFailure::new(
            "binary-content",
            format!("asset cat refuses content type {}", info.content_type),
        ));
    }
    let mut bytes = Vec::new();
    loop {
        let mut chunk = [0; 8192];
        let remaining = (MAX_TEXT_BYTES + 1 - bytes.len()).min(chunk.len());
        let count = a.read(&h, &mut chunk[..remaining]).map_err(host_error)?;
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..count]);
        if bytes.len() > MAX_TEXT_BYTES {
            return Err(AssetFailure::new(
                "too-large",
                "asset cat exceeds 131072 decoded bytes",
            ));
        }
    }
    let text = String::from_utf8(bytes).map_err(|_| {
        AssetFailure::new(
            "invalid-text",
            "asset cat requires UTF-8; no transcoding is performed",
        )
    })?;
    emit(
        out,
        json!({"text": text, "content_type": info.content_type}),
    )
}
pub fn attach(
    a: &impl AssetAccess,
    input: Attachment,
    out: &mut impl Write,
) -> Result<(), AssetFailure> {
    attach_from(
        a,
        input,
        provider::stdin().ok_or_else(|| invalid("asset attach requires piped UTF-8 text"))?,
        out,
    )
}
pub fn attach_from(
    a: &impl AssetAccess,
    input: Attachment,
    mut stdin: impl Read,
    out: &mut impl Write,
) -> Result<(), AssetFailure> {
    validate_type(&input.content_type)?;
    if !input.stdin_piped {
        return Err(invalid("asset attach requires piped UTF-8 text"));
    }
    let mut bytes = Vec::new();
    loop {
        let mut chunk = [0; 8192];
        let remaining = (MAX_TEXT_BYTES + 1 - bytes.len()).min(chunk.len());
        let n = stdin
            .read(&mut chunk[..remaining])
            .map_err(|_| invalid("cannot read piped text"))?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..n]);
        if bytes.len() > MAX_TEXT_BYTES {
            return Err(invalid("stdin text exceeds 131072 UTF-8 bytes"));
        }
    }
    std::str::from_utf8(&bytes).map_err(|_| invalid("asset attach requires UTF-8 text"))?;
    let writer = a
        .allocate(&input.content_type, Encoding::Identity)
        .map_err(host_error)?;
    a.write_all(&writer, &bytes).map_err(host_error)?;
    let handle = a.attach(writer).map_err(host_error)?;
    emit(out, json!({"attached": info_json(a.info(&handle))}))
}
