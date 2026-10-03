# dekopon-provider-asset

Dekopon conversation assets, provider **0.1.0** (SDK prepublication git revision). Command word: `asset`.
Assets stay in broker-owned handles, not data URLs or byte envelopes. No paths, HTTP, storage,
WASI, subprocesses or environment access. `run-command` only proposes; `invoke` calls the host.

```sh
asset ls
asset cat 7
asset rm 7
asset send 7
printf 'hello\n' | asset attach --type text/plain
```

| Command | Capability | Effect / risk | Asset constraint |
|---|---|---|---|
| `ls` | `asset.ls` | read-only / low | none |
| `cat <N>` | `asset.cat` | read-only / low | none |
| `rm <N>` | `asset.rm` | local-write / low | `remove: true` |
| `send <N>` | `asset.send` | external-write / medium | `send: true` |
| `attach --type <mime>` | `asset.attach` | local-write / low | `attach: true` |

`N` is a canonical unsigned 64-bit decimal number. Reference commands propose exactly
`{"source":"chat-asset:7"}` so the gateway discovers a string leaf and passes its descriptor.
Listing metadata does not grant access to an unreferenced asset. Direct invokes enforce closed
schemas independently of the manifest. `ls` takes `{}`; `attach` proposes
`{"content_type":"text/plain","stdin_piped":true}` and reads the bytes only at invocation.
Piped input is required for attach (empty text
is allowed); other commands ignore stdin. Attach accepts any concrete MIME label, not wildcards,
up to 255 ASCII bytes with no control bytes. The label is not content sniffing or conversion.

## Results and limits

- `ls`: `{"assets":[...]}` with metadata (`id`, `content_type`, `encoding`, stored `bytes`,
  `seekable`, `origin`, `sent`). No reads or bytes.
- `cat`: `{"text":"...","content_type":"..."}`. Only `text/*`, application `json`, `xml`,
  `javascript`, `x-www-form-urlencoded`, or application subtypes ending `+json` / `+xml`.
  MIME parameters are accepted. Actual bytes must be UTF-8; no charset transcoding occurs.
  Binary types are refused with `binary-content` naming the content type, before reading.
  At most **131072 decoded bytes**; reads one extra byte to detect overflow, then refuses without
  truncation. Invalid UTF-8 is `invalid-text`. Base64 storage is decoded by the host.
- `rm`: `{"removed":"chat-asset:7"}` after host success. Sent assets cannot be removed.
- `send`: `{"queued":"chat-asset:7"}` after host success, **not proof of delivery**.
- `attach`: `{"attached":{...metadata...}}`, id `null` until the gateway numbers it. At most
  **131072 UTF-8 stdin bytes**; identity storage via allocate → write_all → attach. No automatic
  send, no `attachments` envelope. The gateway appends the numbered asset note.

Each JSON stdout receipt is checked against **1,000,000 serialized bytes** including JSON
escaping and its newline. Lower deployment bounds may still refuse.
SDK errors retain their stable code and bounded message in the provider. The broker may instead
report its sticky `asset-call-rejected` refusal; missing descriptors are refused before invoke.
Nothing retries on failure.
The host separately owns grants, 8 MiB decoded assets, five distinct invocation references,
five attached outputs, and delivery limits. `send` queues this turn's reply only; a failed turn
sends nothing. Duplicate sends are host no-ops; actual delivery and supported MIME types belong
to the gateway transport, not this provider. `allocate` requires configured `assets.rootPath`.

## Example constraint sets

These definitions are not authorization by themselves: bind the intended subject/route through
normal broker policy. Do not grant writes just to permit listing or reading referenced assets.

```yaml
constraintSets:
  asset.ls:
    provider: asset
    effect: read-only
    risk: Low
    constraints: {}
  asset.cat:
    provider: asset
    effect: read-only
    risk: Low
    constraints: {}
  asset.rm:
    provider: asset
    effect: local-write
    risk: Low
    constraints:
      asset: { remove: true }
  asset.send:
    provider: asset
    effect: external-write
    risk: Medium
    constraints:
      asset: { send: true }
  asset.attach:
    provider: asset
    effect: local-write
    risk: Low
    constraints:
      asset: { attach: true }
```

## Build and validate

Rust 1.98.1, wasm32-unknown-unknown. Use the sibling shared provider-workflows checkout:

```sh
cargo fmt --all -- --check
cargo deny check bans licenses sources advisories
cargo clippy --locked --all-targets -- -D warnings
cargo clippy --locked --target wasm32-unknown-unknown --lib -- -D warnings
../provider-workflows/build.sh
DEKOPON_PROVIDER_COMPONENT="$PWD/asset-provider.wasm" cargo test --locked
```

For a worktree, use the absolute path to `provider-workflows/build.sh`.
Native tests inject a private asset seam and cover bounded stdin, operations and effects.
The component test requires `DEKOPON_PROVIDER_COMPONENT` and fails if unset; it exercises real
real typed conformance and host-level refusal. Successful descriptor/effect paths are native
fake tests, not claimed as end-to-end gateway delivery tests. Shared CI checks imports and
reproducibility. Release tags publish `ghcr.io/dekopon-agents/provider-asset:0.1.0`.
