# Security policy

Report suspected vulnerabilities privately through GitHub's security-advisory flow for
`dekopon-agents/dekopon-provider-asset`. Do not put private conversation content in public issues.
Published release bytes are immutable; fixes ship as new versions.

## Authority

The component imports `dekopon:asset/asset@0.1.0` and `dekopon:stdio/streams@0.1.0`.
`run-command` parses argv and the piped-input marker without reading stdin; after authorization,
`invoke` opens exact `chat-asset:<N>` references passed by the gateway. Listing metadata
does not authorize opening unreferenced files. The broker owns attach/remove/send grants and
transactional effects. Attaching never sends; send success only means queued for this turn,
not delivered. No filesystem paths, HTTP, durable storage, WASI, environment or subprocesses.

Input schemas are closed and enforced in Rust, not trusted to the host. The SDK parses wire JSON
first; duplicate names have serde_json's last-value behavior and cannot be detected afterward.
Attach stdin is at most 131072 UTF-8 bytes. MIME labels are concrete, bounded to 255 ASCII bytes,
and contain no controls; they are declarations, not content conversion or sniffing.

Cat only reads approved text MIME types, validates UTF-8 and refuses above 131072 decoded bytes,
reading at most one extra byte. It never trusts advertised stored length for allocation. It
writes text intentionally to stdout; callers must authorize that disclosure. JSON escaping
is accounted for by checking the stdout receipt below 1,000,000 bytes. The broker
still enforces its independently configured input/output, fuel, memory, time and asset limits.
Host errors are propagated without retry. Native fake tests prove successful asset operations;
the testkit component suite proves pure proposals and fail-closed asset access, not live delivery.

## Supply chain

SDK/testkit at the prepublication core git revision, Cargo-generated lock and shared
provider-workflows CI/release. CI runs fmt, deny, both clippy targets, component build,
native/component tests and reproducibility validation. Release v0.1.0 publishes the component
as the single application/wasm layer of its immutable OCI manifest. The shared workflow attests
file subjects (component and SBOM), not the OCI manifest: verify the component attestation and
bind its SHA-256 to the immutable manifest's sole layer. No direct manifest-attestation claim.
