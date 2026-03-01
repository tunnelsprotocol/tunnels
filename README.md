# Tunnels Protocol

A purpose-built blockchain for recording contributions and distributing revenue - for humans and AI agents alike.

Tunnels tracks who did what work on a project, verifies it through an open challenge window, and splits revenue deterministically based on attested contribution weight. No middlemen, no negotiation, no discretion. The same inputs always produce the same outputs.

Identities are cryptographic key pairs. Contribution history is non-transferable - you can't buy someone else's track record. Attestations are immutable after finalization. Distribution parameters are visible at project creation and can never be changed. The protocol self-polices through economic incentive: every fraudulent attestation dilutes existing contributors, who have direct financial motivation to challenge it.

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

## Documentation

- [Whitepaper](docs/tunnels-protocol-whitepaper.pdf)
- [JSON-RPC API](docs/json-rpc-api.md)
- [Running a Node](docs/running-a-node.md)
- [Key Generator](docs/tunnels-keygen.html) - generate keys in your browser, no install required

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.

See [LICENSE-APACHE](LICENSE-APACHE) and [LICENSE-MIT](LICENSE-MIT).
