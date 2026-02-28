# Tunnels Protocol

A protocol for recording contributions and distributing revenue. Tunnels enables transparent, auditable tracking of who contributed what to a project and automatic distribution of revenue based on those contributions.

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

## License

Licensed under either of Apache License, Version 2.0 or MIT license at your option.

See [LICENSE-APACHE](LICENSE-APACHE) and [LICENSE-MIT](LICENSE-MIT).
