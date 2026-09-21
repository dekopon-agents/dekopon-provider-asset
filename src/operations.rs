use crate::{ATTACH, CAT, LS, MAX_ENVELOPE_BYTES, MAX_TEXT_BYTES, RM, SEND, invalid};
use dekopon_provider_sdk::{
    ProviderError,
    asset::{self, AssetError, Encoding, Info},
};
use serde::Deserialize;
use serde_json::{Value, json};

/// Private seam; the production implementation is exactly the SDK's handle operations.
pub(crate) trait Assets {
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
pub(crate) struct Host;
impl Assets for Host {
    type Handle = asset::Handle;
    type Writer = asset::Writer;
    fn list(&self) -> Vec<Info> {
        asset::list()
    }
    fn open(&self, reference: &str) -> Result<Self::Handle, AssetError> {
        asset::open(reference)
    }
    fn info(&self, handle: &Self::Handle) -> Info {
        handle.info()
    }
    fn read(&self, handle: &Self::Handle, buffer: &mut [u8]) -> Result<usize, AssetError> {
        handle.read(buffer)
    }
    fn remove(&self, handle: &Self::Handle) -> Result<(), AssetError> {
        asset::remove(handle)
    }
    fn send(&self, handle: &Self::Handle) -> Result<(), AssetError> {
        asset::send(handle)
    }
    fn allocate(&self, content_type: &str, encoding: Encoding) -> Result<Self::Writer, AssetError> {
        asset::allocate(content_type, encoding)
    }
    fn write_all(&self, writer: &Self::Writer, bytes: &[u8]) -> Result<(), AssetError> {
        writer.write_all(bytes)
    }
    fn attach(&self, writer: Self::Writer) -> Result<Self::Handle, AssetError> {
        asset::attach(writer)
    }
}
fn host_error(error: AssetError) -> ProviderError {
    ProviderError::new(error.code.as_str(), error.message)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Empty {}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    source: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Attachment {
    content_type: String,
    text: String,
}

pub(crate) fn validate_reference(reference: &str) -> Result<(), ProviderError> {
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

fn media_type(value: &str) -> Result<mime::Mime, ProviderError> {
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

pub(crate) fn validate_attach(content_type: &str, text: &str) -> Result<(), ProviderError> {
    media_type(content_type)?;
    if text.len() > MAX_TEXT_BYTES {
        return Err(invalid("stdin text exceeds 131072 UTF-8 bytes"));
    }
    Ok(())
}

fn info_json(info: Info) -> Value {
    json!({"id": info.id, "content_type": info.content_type,
        "encoding": match info.encoding { Encoding::Identity => "identity", Encoding::Base64 => "base64" },
        "bytes": info.stored_bytes, "seekable": info.seekable, "origin": info.origin, "sent": info.sent})
}

fn bounded(output: Value) -> Result<Value, ProviderError> {
    // Match the SDK's actual success wrapper, not just the inner text length.
    let envelope = dekopon_provider_sdk::ComponentResponse::Succeeded {
        output: output.clone(),
    };
    let length = serde_json::to_vec(&envelope)
        .map_err(|_| ProviderError::new("internal", "output serialization failed"))?
        .len();
    if length > MAX_ENVELOPE_BYTES {
        return Err(ProviderError::new(
            "too-large",
            "serialized output exceeds provider bound",
        ));
    }
    Ok(output)
}

pub(crate) fn invoke<A: Assets>(
    assets: &A,
    capability: &str,
    input: Value,
) -> Result<Value, ProviderError> {
    // Serde's struct deserializer also accepts positional arrays; closed schemas do not.
    if matches!(capability, LS | RM | SEND | CAT | ATTACH) && !input.is_object() {
        return Err(invalid("asset capability input must be an object"));
    }
    let output = match capability {
        LS => {
            serde_json::from_value::<Empty>(input)
                .map_err(|_| invalid("asset.ls expects an empty object"))?;
            json!({"assets": assets.list().into_iter().map(info_json).collect::<Vec<_>>()})
        }
        RM | SEND | CAT => {
            let Source { source } = serde_json::from_value(input)
                .map_err(|_| invalid("expected only a source string"))?;
            validate_reference(&source)?;
            let handle = assets.open(&source).map_err(host_error)?;
            match capability {
                RM => {
                    assets.remove(&handle).map_err(host_error)?;
                    json!({"removed": source})
                }
                SEND => {
                    assets.send(&handle).map_err(host_error)?;
                    json!({"queued": source})
                }
                _ => cat(assets, &handle)?,
            }
        }
        ATTACH => {
            let Attachment { content_type, text } = serde_json::from_value(input)
                .map_err(|_| invalid("expected only content_type and text strings"))?;
            validate_attach(&content_type, &text)?;
            let writer = assets
                .allocate(&content_type, Encoding::Identity)
                .map_err(host_error)?;
            assets
                .write_all(&writer, text.as_bytes())
                .map_err(host_error)?;
            let handle = assets.attach(writer).map_err(host_error)?;
            json!({"attached": info_json(assets.info(&handle))})
        }
        _ => {
            return Err(ProviderError::new(
                "unsupported-capability",
                "unknown asset capability",
            ));
        }
    };
    bounded(output)
}

fn cat<A: Assets>(assets: &A, handle: &A::Handle) -> Result<Value, ProviderError> {
    let info = assets.info(handle);
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
        return Err(ProviderError::new(
            "binary-content",
            format!("asset cat refuses content type {}", info.content_type),
        ));
    }
    // Never reserve the advertised stored length or call read_all: it may be encoded or unknown.
    // Read at most one decoded byte beyond the bound, then refuse without partial output.
    let mut bytes = Vec::new();
    loop {
        let mut chunk = [0; 8192];
        let remaining = (MAX_TEXT_BYTES + 1 - bytes.len()).min(chunk.len());
        let count = assets
            .read(handle, &mut chunk[..remaining])
            .map_err(host_error)?;
        if count == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..count]);
        if bytes.len() > MAX_TEXT_BYTES {
            return Err(ProviderError::new(
                "too-large",
                "asset cat exceeds 131072 decoded bytes",
            ));
        }
    }
    let text = String::from_utf8(bytes).map_err(|_| {
        ProviderError::new(
            "invalid-text",
            "asset cat requires UTF-8; no transcoding is performed",
        )
    })?;
    Ok(json!({"text": text, "content_type": info.content_type}))
}
