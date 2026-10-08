# Changelog

## [Unreleased]

## [0.2.1] - 2026-10-08

- Pin the asset provider SDK, testkit, broker host and carried Dekopon dependencies to 0.36.0; preserve the asset command and WIT contracts.

## [0.2.0] - 2026-10-03

- Migrate asset commands to the typed Dekopon 0.31.0 SDK and stdio streams.
- Keep broker-owned asset handles, bounded text and stdin attachment semantics; validate the real component and broker refusal.

## [0.1.0] - 2026-09-21

- First asset provider release on Dekopon SDK 0.18.0: `asset ls`, `rm`, `send`, `cat`, and `attach`.
- Pure command proposals preserve exact asset references for gateway descriptor discovery.
- Bounded UTF-8 text reads and stdin attachment through broker-owned handles; attach is not send.
- Native injected asset tests, real component proposal/refusal tests, and shared CI/release gates.
