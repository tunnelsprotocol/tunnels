# Tunnels JSON-RPC API Reference

This document provides a complete reference for all JSON-RPC methods supported by the Tunnels node.

## Overview

The Tunnels node exposes a JSON-RPC 2.0 API over HTTP. By default, the RPC server listens on `127.0.0.1:9334`.

### Making Requests

All requests use HTTP POST with `Content-Type: application/json`:

```bash
curl -X POST http://localhost:9334 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "method": "getBlockCount",
    "params": [],
    "id": 1
  }'
```

### Response Format

Successful responses:
```json
{
  "jsonrpc": "2.0",
  "result": <value>,
  "id": 1
}
```

Error responses:
```json
{
  "jsonrpc": "2.0",
  "error": {
    "code": -32001,
    "message": "Block not found"
  },
  "id": 1
}
```

## Error Codes

| Code | Meaning |
|------|---------|
| -32700 | Parse error (invalid JSON) |
| -32600 | Invalid request |
| -32601 | Method not found |
| -32602 | Invalid params |
| -32603 | Internal error |
| -32001 | Resource not found (block, tx, etc.) |
| -32003 | Transaction rejected |

---

## Chain Methods

### getBlockCount

Returns the height of the current best chain (number of blocks - 1).

**Parameters:** None

**Returns:** `number` - Current block height

**Example:**
```bash
curl -s localhost:9334 -d '{
  "jsonrpc": "2.0",
  "method": "getBlockCount",
  "params": [],
  "id": 1
}'
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "result": 12345,
  "id": 1
}
```

---

### getBlockHash

Returns the hash of the block at the specified height.

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| height | number | Block height |

**Returns:** `string` - Block hash (64 hex characters)

**Example:**
```bash
curl -s localhost:9334 -d '{
  "jsonrpc": "2.0",
  "method": "getBlockHash",
  "params": [100],
  "id": 1
}'
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "result": "004f22d54de1821c99bac8a9a1ab516e0f6a7cd2aaceb69836aea4823427c2e6",
  "id": 1
}
```

---

### getBlock

Returns full block information for the given hash.

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| hash | string | Block hash (64 hex characters) |

**Returns:** `BlockInfo` object

**BlockInfo Fields:**
| Field | Type | Description |
|-------|------|-------------|
| hash | string | Block hash |
| height | number | Block height |
| version | number | Protocol version |
| prev_block_hash | string | Previous block hash |
| tx_root | string | Merkle root of transactions |
| state_root | string | Protocol state root |
| timestamp | number | Unix timestamp |
| difficulty | number | Block difficulty |
| nonce | number | PoW nonce |
| tx_count | number | Number of transactions |
| transactions | string[] | Transaction IDs |

**Example:**
```bash
curl -s localhost:9334 -d '{
  "jsonrpc": "2.0",
  "method": "getBlock",
  "params": ["004f22d54de1821c99bac8a9a1ab516e0f6a7cd2aaceb69836aea4823427c2e6"],
  "id": 1
}'
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "result": {
    "hash": "004f22d54de1821c99bac8a9a1ab516e0f6a7cd2aaceb69836aea4823427c2e6",
    "height": 0,
    "version": 1,
    "prev_block_hash": "0000000000000000000000000000000000000000000000000000000000000000",
    "tx_root": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    "state_root": "...",
    "timestamp": 1700000000,
    "difficulty": 8,
    "nonce": 256,
    "tx_count": 0,
    "transactions": []
  },
  "id": 1
}
```

---

### getBestBlockHash

Returns the hash of the current best (tip) block.

**Parameters:** None

**Returns:** `string` - Block hash

**Example:**
```bash
curl -s localhost:9334 -d '{
  "jsonrpc": "2.0",
  "method": "getBestBlockHash",
  "params": [],
  "id": 1
}'
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "result": "00abc123...",
  "id": 1
}
```

---

### getTransaction

Returns transaction information by ID.

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| txid | string | Transaction ID (64 hex characters) |

**Returns:** `TransactionInfo` object

**TransactionInfo Fields:**
| Field | Type | Description |
|-------|------|-------------|
| id | string | Transaction ID |
| signer | string | Signer's public key |
| signature | string | Ed25519 signature |
| pow_nonce | number | PoW nonce |
| pow_hash | string | PoW hash |
| tx_type | string | Transaction type name |

**Example:**
```bash
curl -s localhost:9334 -d '{
  "jsonrpc": "2.0",
  "method": "getTransaction",
  "params": ["a1b2c3d4..."],
  "id": 1
}'
```

---

## Mempool Methods

### submitTransaction

Submits a signed transaction to the mempool.

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| tx_hex | string | Serialized signed transaction (hex) |

**Returns:** `string` - Transaction ID if accepted

**Example:**
```bash
curl -s localhost:9334 -d '{
  "jsonrpc": "2.0",
  "method": "submitTransaction",
  "params": ["<hex-encoded-signed-transaction>"],
  "id": 1
}'
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "result": "a1b2c3d4e5f6...",
  "id": 1
}
```

**Error Cases:**
- `-32602`: Invalid hex encoding or transaction format
- `-32003`: Transaction rejected (invalid signature, insufficient PoW, state error)

---

### getMempoolInfo

Returns information about the transaction mempool.

**Parameters:** None

**Returns:** `MempoolInfo` object

**MempoolInfo Fields:**
| Field | Type | Description |
|-------|------|-------------|
| size | number | Number of transactions |
| bytes | number | Estimated total size in bytes |

**Example:**
```bash
curl -s localhost:9334 -d '{
  "jsonrpc": "2.0",
  "method": "getMempoolInfo",
  "params": [],
  "id": 1
}'
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "result": {
    "size": 42,
    "bytes": 21000
  },
  "id": 1
}
```

---

### getRawMempool

Returns all transaction IDs in the mempool.

**Parameters:** None

**Returns:** `string[]` - Array of transaction IDs

**Example:**
```bash
curl -s localhost:9334 -d '{
  "jsonrpc": "2.0",
  "method": "getRawMempool",
  "params": [],
  "id": 1
}'
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "result": ["a1b2c3...", "d4e5f6..."],
  "id": 1
}
```

---

## Mining Methods

### mine

Mines the specified number of blocks (devnet only).

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| count | number | Number of blocks to mine (default: 1) |

**Returns:** `string[]` - Array of mined block hashes

**Example:**
```bash
curl -s localhost:9334 -d '{
  "jsonrpc": "2.0",
  "method": "mine",
  "params": [1],
  "id": 1
}'
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "result": ["00abc123..."],
  "id": 1
}
```

**Note:** Only available when the node is started with `--devnet --mine`.

---

### getBlockTemplate

Returns a block template for external mining.

**Parameters:** None

**Returns:** `BlockTemplate` object

**BlockTemplate Fields:**
| Field | Type | Description |
|-------|------|-------------|
| height | number | Height of block to mine |
| prev_block_hash | string | Previous block hash |
| difficulty | number | Required difficulty |
| timestamp | number | Suggested timestamp |
| transactions | string[] | Transaction IDs to include |

**Example:**
```bash
curl -s localhost:9334 -d '{
  "jsonrpc": "2.0",
  "method": "getBlockTemplate",
  "params": [],
  "id": 1
}'
```

---

### getMiningInfo

Returns mining status information.

**Parameters:** None

**Returns:** `MiningInfo` object

**MiningInfo Fields:**
| Field | Type | Description |
|-------|------|-------------|
| height | number | Current height |
| difficulty | number | Current difficulty |
| devnet | boolean | Whether devnet mode is enabled |
| mining_enabled | boolean | Whether mining is enabled |

---

## Network Methods

### getPeerInfo

Returns information about connected peers.

**Parameters:** None

**Returns:** `PeerInfo[]` - Array of peer information

**PeerInfo Fields:**
| Field | Type | Description |
|-------|------|-------------|
| id | string | Peer ID |
| addr | string | Peer address |
| direction | string | "inbound" or "outbound" |
| best_height | number | Peer's best known height |

**Example:**
```bash
curl -s localhost:9334 -d '{
  "jsonrpc": "2.0",
  "method": "getPeerInfo",
  "params": [],
  "id": 1
}'
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "result": [
    {
      "id": "abc123...",
      "addr": "192.168.1.100:9333",
      "direction": "outbound",
      "best_height": 12345
    }
  ],
  "id": 1
}
```

---

### getConnectionCount

Returns the total number of connections.

**Parameters:** None

**Returns:** `number` - Connection count

---

### addPeer

Attempts to add a peer connection.

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| address | string | Peer address (ip:port) |

**Returns:** `boolean` - Whether the request was accepted

---

### getNodeInfo

Returns general node information.

**Parameters:** None

**Returns:** `NodeInfo` object

**NodeInfo Fields:**
| Field | Type | Description |
|-------|------|-------------|
| version | string | Node software version |
| protocol_version | number | Protocol version |
| network | string | "mainnet" or "devnet" |
| height | number | Current block height |
| best_block_hash | string | Best block hash |
| connections | number | Total connections |
| synced | boolean | Whether node is synced |

**Example:**
```bash
curl -s localhost:9334 -d '{
  "jsonrpc": "2.0",
  "method": "getNodeInfo",
  "params": [],
  "id": 1
}'
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "result": {
    "version": "0.1.0",
    "protocol_version": 1,
    "network": "mainnet",
    "height": 12345,
    "best_block_hash": "00abc123...",
    "connections": 8,
    "synced": true
  },
  "id": 1
}
```

---

## Protocol State Methods

### get_identity

Looks up an identity by public key.

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| public_key | string | Ed25519 public key (64 hex characters) |

**Returns:** `IdentityInfo | null`

**IdentityInfo Fields:**
| Field | Type | Description |
|-------|------|-------------|
| address | string | Derived address (40 hex) |
| public_key | string | Public key |
| identity_type | string | "human" or "agent" |
| parent | string? | Parent address (for agents) |
| profile_hash | string? | Profile metadata hash |
| active | boolean | Whether identity is active |

**Example:**
```bash
curl -s localhost:9334 -d '{
  "jsonrpc": "2.0",
  "method": "get_identity",
  "params": ["dc99512e742eebc93ccecc2aa2d007a13e8b522ecdf7a16455913a2e0efcc2ea"],
  "id": 1
}'
```

---

### get_project

Looks up a project by ID.

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| project_id | string | Project ID (40 hex characters) |

**Returns:** `ProjectInfo | null`

**ProjectInfo Fields:**
| Field | Type | Description |
|-------|------|-------------|
| project_id | string | Project ID |
| creator | string | Creator's address |
| rho | number | Reserve fee (basis points) |
| phi | number | Application fee (basis points) |
| gamma | number | Genesis fee (basis points) |
| delta | number | Predecessor fee (basis points) |

---

### get_attestation

Looks up an attestation by ID.

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| attestation_id | string | Attestation ID (40 hex characters) |

**Returns:** `AttestationInfo | null`

**AttestationInfo Fields:**
| Field | Type | Description |
|-------|------|-------------|
| attestation_id | string | Attestation ID |
| project_id | string | Project ID |
| contributor | string | Contributor's address |
| units | number | Labor units |
| evidence_hash | string | Evidence hash |
| bond_denomination | string | Bond denomination |
| created_at | number | Creation timestamp |
| status | string | "pending", "challenged", "finalized", "rejected" |
| finalized_at | number? | Finalization timestamp |

---

### get_challenge

Looks up a challenge by ID.

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| challenge_id | string | Challenge ID (40 hex characters) |

**Returns:** `ChallengeInfo | null`

**ChallengeInfo Fields:**
| Field | Type | Description |
|-------|------|-------------|
| challenge_id | string | Challenge ID |
| attestation_id | string | Challenged attestation |
| challenger | string | Challenger's address |
| evidence_hash | string | Challenge evidence hash |
| created_at | number | Creation timestamp |
| status | string | "open", "responded", "resolved_valid", "resolved_invalid" |

---

### get_balance

Queries claimable balance for an identity.

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| public_key | string | Identity's public key |
| project_id | string | Project ID |

**Returns:** `BalanceInfo` object

**BalanceInfo Fields:**
| Field | Type | Description |
|-------|------|-------------|
| claimable | string | Claimable amount (U256 as hex) |

**Example:**
```bash
curl -s localhost:9334 -d '{
  "jsonrpc": "2.0",
  "method": "get_balance",
  "params": ["dc99512e...", "a1b2c3d4..."],
  "id": 1
}'
```

---

### get_accumulator

Queries reward accumulator state for a project.

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| project_id | string | Project ID |

**Returns:** `AccumulatorInfo | null`

**AccumulatorInfo Fields:**
| Field | Type | Description |
|-------|------|-------------|
| total_finalized_units | number | Total finalized labor units |
| reward_per_unit_global | string | Global reward per unit (U256 as hex) |

---

### get_reserve_balance

Queries project reserve balance.

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| project_id | string | Project ID |

**Returns:** `ReserveInfo` object

**ReserveInfo Fields:**
| Field | Type | Description |
|-------|------|-------------|
| balance | string | Reserve balance (U256 as hex) |

---

### get_attestations_by_identity

Returns all attestations for an identity.

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| public_key | string | Identity's public key |

**Returns:** `AttestationInfo[]`

---

### get_attestations_by_project

Returns all attestations for a project.

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| project_id | string | Project ID |

**Returns:** `AttestationInfo[]`

---

### get_projects_by_identity

Returns all projects owned by an identity.

**Parameters:**
| Name | Type | Description |
|------|------|-------------|
| public_key | string | Identity's public key |

**Returns:** `ProjectInfo[]`

---

## Transaction Lifecycle

This section explains how to construct, sign, and submit transactions.

### Step 1: Generate a Keypair

Use the `tunnels-keygen` tool:

```bash
tunnels-keygen --json
```

Output:
```json
{
  "public_key": "dc99512e742eebc93ccecc2aa2d007a13e8b522ecdf7a16455913a2e0efcc2ea",
  "private_key": "301ec0101a2471b767d1df59367664bd96c5a64a29af15787a2a63a93395cd17",
  "address": "7d02e6967109f9c7054238fc37b2b64abc269138"
}
```

### Step 2: Create an Identity

To interact with the protocol, first create an identity:

```javascript
// Transaction structure (pseudocode)
const tx = {
  type: "CreateIdentity",
  public_key: "<your-public-key>",
  identity_type: "Human",
  profile_hash: null
};

// 1. Serialize transaction
const tx_bytes = serialize(tx);

// 2. Sign with Ed25519
const signature = ed25519_sign(private_key, tx_bytes);

// 3. Mine proof-of-work
const { pow_nonce, pow_hash } = mine_pow(tx, public_key, signature, difficulty);

// 4. Create signed transaction
const signed_tx = {
  tx,
  signer: public_key,
  signature,
  pow_nonce,
  pow_hash
};

// 5. Serialize and hex-encode
const tx_hex = hex_encode(serialize(signed_tx));
```

### Step 3: Submit the Transaction

```bash
curl -s localhost:9334 -d '{
  "jsonrpc": "2.0",
  "method": "submitTransaction",
  "params": ["<tx_hex>"],
  "id": 1
}'
```

### Step 4: Wait for Confirmation

Check that the transaction is in a block:

```bash
# Check mempool (should disappear after mining)
curl -s localhost:9334 -d '{
  "jsonrpc": "2.0",
  "method": "getRawMempool",
  "params": [],
  "id": 1
}'

# Query identity (should exist after confirmation)
curl -s localhost:9334 -d '{
  "jsonrpc": "2.0",
  "method": "get_identity",
  "params": ["<public_key>"],
  "id": 1
}'
```

### Complete Example: Create Identity and Project

```bash
#!/bin/bash
# Complete lifecycle example using curl

RPC="http://localhost:9334"

# Generate keypair (save these!)
KEYPAIR=$(tunnels-keygen --json)
PUBLIC_KEY=$(echo $KEYPAIR | jq -r .public_key)
PRIVATE_KEY=$(echo $KEYPAIR | jq -r .private_key)
ADDRESS=$(echo $KEYPAIR | jq -r .address)

echo "Public Key: $PUBLIC_KEY"
echo "Address: $ADDRESS"

# Note: In a real application, you would:
# 1. Build the transaction using the Tunnels SDK
# 2. Sign it with the private key
# 3. Mine the PoW
# 4. Serialize and submit

# For devnet testing, transactions can be submitted directly
# using the SDK or by constructing the hex manually

# Check identity exists
curl -s $RPC -d "{
  \"jsonrpc\": \"2.0\",
  \"method\": \"get_identity\",
  \"params\": [\"$PUBLIC_KEY\"],
  \"id\": 1
}" | jq

# Check projects
curl -s $RPC -d "{
  \"jsonrpc\": \"2.0\",
  \"method\": \"get_projects_by_identity\",
  \"params\": [\"$PUBLIC_KEY\"],
  \"id\": 1
}" | jq
```

## Transaction Types

The protocol supports 13 transaction types:

| Type | Description |
|------|-------------|
| CreateIdentity | Register a new human identity |
| CreateAgent | Create an agent identity under a human |
| DeactivateAgent | Deactivate an agent |
| UpdateProfileHash | Update identity profile metadata |
| RegisterProject | Register a new project |
| SubmitAttestation | Submit a labor attestation |
| ChallengeAttestation | Challenge a pending attestation |
| RespondToChallenge | Respond to a challenge |
| ResolveChallenge | Resolve a challenge (arbiter) |
| FinalizeAttestation | Finalize a pending attestation |
| DepositRevenue | Deposit revenue into a project |
| ClaimRevenue | Claim earned revenue |
| ClaimReserve | Claim from project reserve |

Refer to the SDK documentation for transaction construction details.
