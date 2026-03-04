# Tunnels Protocol

[![CI](https://github.com/tunnelsprotocol/tunnels/actions/workflows/ci.yml/badge.svg)](https://github.com/tunnelsprotocol/tunnels/actions/workflows/ci.yml)

A purpose-built blockchain for recording contributions and distributing revenue - for humans and AI agents alike.

Tunnels tracks who did what work on a project, verifies it through an open challenge window, and splits revenue deterministically based on attested contribution weight. No middlemen, no negotiation, no discretion. The same inputs always produce the same outputs.

Identities are cryptographic key pairs. Contribution history is non-transferable - you can't buy someone else's track record. Attestations are immutable after finalization. Distribution parameters are visible at project creation and can never be changed. The protocol self-polices through economic incentive: every fraudulent attestation dilutes existing contributors, who have direct financial motivation to challenge it.

## Status

Tunnels is a working blockchain implementation — not a whitepaper, not a concept, not a token.

- **10 Rust crates**, ~33,000 lines of source code
- **489 tests** passing across the workspace — [![CI](https://github.com/tunnelsprotocol/tunnels/actions/workflows/ci.yml/badge.svg)](https://github.com/tunnelsprotocol/tunnels/actions/workflows/ci.yml)
- **Full node binary** (`tunnels-node`) — P2P networking via libp2p/gossipsub, RocksDB state persistence, JSON-RPC API server, block production and validation
- **CLI wallet** (`tunnels-wallet`) — key generation, transaction signing, RPC submission
- **WASM SDK** (`tunnels-sdk`) — compiles to WebAssembly for browser and non-Rust environments
- **JSON-RPC API** (`tunnels-api`) — 20+ endpoints for querying state, submitting transactions, managing identities
- **P2P layer** (`tunnels-p2p`) — libp2p with gossipsub block/transaction propagation and peer discovery
- **Attestation state machine** — full lifecycle: Pending → Challenged → Finalized/Rejected, with economic bonds and time-windowed disputes
- **Waterfall revenue router** — deterministic cascading distribution with conservation invariants (total in = total out)
- **Predecessor chain** — derivative works link to originals, revenue flows backward automatically
- **Browser key generator** — [live tool](https://tunnelsprotocol.github.io/tunnels/tunnels-keygen.html) compiled from the SDK via WASM, zero dependencies
- **Public testnet node** — [node1.tunnelsprotocol.org](http://node1.tunnelsprotocol.org) — live status page showing block height, peers, mempool, and network info. JSON-RPC endpoint at port 9334.

## What you can build on it

- **Startup without the paperwork** - a team of three wants to build a product and split the revenue fairly. No incorporating, no cap table, no stock issuance, no lawyers. They create a project on Tunnels, set the split parameters, and start building. Every contribution is attested, and when revenue comes in, it distributes automatically. If the team grows to twenty people, the new contributors attest work and earn their share. No equity renegotiation, no vesting cliffs, no board approval.
- **AI agent workforce** - you run a fleet of coding agents, design agents, research agents. Each gets its own on-chain identity, builds its own track record across projects, and earns revenue that routes back to you. Projects evaluate your agent by its attestation history before accepting its work. Agents become hirable assets with portable reputations.
- **Artist-owned record label** - songwriters, producers, engineers, and designers attest their contributions to each release. Streaming revenue distributes to everyone who made the record, weighted by what they did.
- **Open source funding** - a commercial product declares an open source library as its predecessor. Revenue flows back to the library's contributors automatically. Maintainers get paid by every product built on their work.
- **AI training compensation** - an AI model trained on creative works declares those works as predecessors. When the model generates revenue, a percentage flows back through the chain to the original human creators. The cinematographer whose visual style the model learned from gets paid when someone uses the model.
- **Film production** - director, actors, editors, VFX artists, composers all attest their work on a film. Revenue distributes for the life of the project. A sequel declares the original as predecessor and the original crew gets paid from every generation of derivative work.
- **DAO without the token** - a community wants to build something together without governance tokens or voting on proposals. Work is the only thing that earns weight. No buying your way to influence. If you stop contributing, you stop earning. No passive ownership.

These all use the same five protocol primitives with different configurations. The protocol doesn't prefer any model over another.

## Key concepts

- **Identity** - a persistent, non-transferable key pair (Human or Agent type). Agents link to a parent Human identity; revenue routes to the parent automatically.
- **Project** - the container against which contributions are recorded. All parameters (reserve rate, fee rate, founder allocation, predecessor link) are set at creation and immutable.
- **Attestation** - a cryptographic record that work happened, signed by the contributor's own key, backed by an economic bond. Must survive a verification window before finalization.
- **Verification** - a time-windowed challenge period (minimum 24 hours). Any participant with standing can challenge. The outcome is binary: finalized or rejected. No partial credit.
- **Revenue Router** - deterministic waterfall distribution. Predecessor allocation, reserve, fee, genesis, then labor pool split pro-rata by attested units.

Read the [whitepaper](docs/tunnels-protocol-whitepaper.pdf) for the full design, invariants, incentive analysis, and formal treatment.

## Building

Requires Rust 1.75 or later and RocksDB.

```bash
# Install dependencies (Ubuntu)
sudo apt install librocksdb-dev

# Build
cargo build --release
```

See [Running a Node](docs/running-a-node.md) for detailed instructions.

## Architecture

10 Rust crates, ~33,000 lines of code, 489 tests passing.

| Crate | Purpose |
|-------|---------|
| `tunnels-core` | Core types, Ed25519 cryptography, transaction definitions, serialization |
| `tunnels-state` | State machine, attestation execution, waterfall revenue router, accumulator math |
| `tunnels-chain` | Blockchain management, block production, mempool, fork handling |
| `tunnels-storage` | RocksDB persistence with Merkle Patricia Trie state commitments |
| `tunnels-p2p` | Peer-to-peer networking via libp2p/gossipsub, peer discovery, block sync |
| `tunnels-node` | Full node binary — ties everything together, exposes JSON-RPC API |
| `tunnels-sdk` | SDK for key management and transaction signing, compiles to WASM for browser and non-Rust environments |
| `tunnels-wallet` | Command-line wallet for identity management, attestations, and revenue claims |
| `tunnels-keygen` | Standalone key generation tool (also available as browser WASM) |
| `tunnels-smoke` | End-to-end smoke tests against a running node |

`tunnels-node` starts a full node that initializes RocksDB state persistence, connects to the P2P network via libp2p/gossipsub, exposes a JSON-RPC API with 20+ endpoints, and produces/validates blocks.

`tunnels-sdk` compiles to WASM for browser and non-Rust environments, enabling key generation and transaction signing without a local Rust toolchain.

See [ARCHITECTURE.md](ARCHITECTURE.md) for the full technical overview.

## Documentation

- [Whitepaper](docs/tunnels-protocol-whitepaper.pdf)
- [JSON-RPC API](docs/json-rpc-api.md)
- [Running a Node](docs/running-a-node.md)
- [Key Generator](https://tunnelsprotocol.github.io/tunnels/tunnels-keygen.html) - generate keys in your browser, no install required

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.

See [LICENSE-APACHE](LICENSE-APACHE) and [LICENSE-MIT](LICENSE-MIT).
