# net-proxy-rs

A functional-style split proxy prototype with rule-based dispatch, SOCKS5/TCP upstreams, and platform adapters.

## Quick start (CLI)
1. Create a configuration file `proxifier.yaml` using the schema shown in `docs/ARCHITECTURE.md`.
2. Run the CLI dispatcher:
   ```bash
   cargo run -- --config proxifier.yaml run
   ```
3. Test rule evaluation without starting listeners:
   ```bash
   cargo run -- --config proxifier.yaml test-rule --domain example.com --ip 93.184.216.34:80
   ```

## Features
- Rule engine supporting IP CIDR, domain wildcards, and process path filters.
- Upstreams: direct, TCP forward with header preamble, SOCKS5 (no-auth and username/password).
- Platform abstraction (`PacketSource`) with Windows WFP callout stub and mock generator for tests.
- Transparent TCP piping built on Tokio `copy_bidirectional`.
- Optional GUI hook (feature flag `gui`) for future desktop integrations.

See `docs/ARCHITECTURE.md` for a detailed design overview.
