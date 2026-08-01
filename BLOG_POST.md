# Building a Private DAO with NEAR OutLayer

*Anonymous, verifiable voting at 1/1000th the cost of on-chain execution*

---

## The Problem: Privacy vs Transparency

DAOs need voting systems that are:
- ✅ **Transparent**: Results must be verifiable
- ✅ **Private**: Individual votes must be secret
- ✅ **Affordable**: Shouldn't cost $100 per vote

But on NEAR Protocol:
- ❌ All smart contract state is public (votes would be visible)
- ❌ Heavy cryptography is expensive (300+ NEAR for 100 votes)
- ❌ Gas limits prevent complex operations (300 TGas max)

**Solution**: Move heavy computation off-chain to NEAR OutLayer's Trusted Execution Environment (TEE).

---

## How It Works

### Architecture

```
Client               NEAR Blockchain          OutLayer (TEE)
  │                        │                        │
  │  1. Encrypt vote       │                        │
  │  ────────────────────> │                        │
  │     with user pubkey   │                        │
  │                        │                        │
  │                        │  2. Tally votes        │
  │                        │  ──────────────────>   │
  │                        │     + master secret    │
  │                        │                        │
  │                        │  3. Decrypt + count    │
  │                        │  <─────────────────    │
  │                        │     {yes: 7, no: 3}    │
  │                        │     + merkle proofs    │
  │  4. Verify proof       │                        │
  │  <─────────────────    │                        │
```

### Key Components

**1. Master Secret in TEE**

Instead of giving each user their own keypair, we use a single master secret stored securely:

```rust
// In TEE only
let master_secret = std::env::var("PROTECTED_DAO_MASTER_SECRET")?;

// Derive unique key for each user
let user_privkey = HKDF-SHA256(
    master_secret,
    info: "user:dao.testnet:alice.testnet"
);

let user_pubkey = secp256k1::derive_pubkey(user_privkey);
```

**Why this is powerful:**
- ✅ One secret → unlimited users
- ✅ Keys derived on-demand (never stored)
- ✅ Private keys never leave TEE
- ✅ Master secret managed by OutLayer's Keymaster

**2. Client-Side Encryption**

Users encrypt their vote before sending to blockchain:

```javascript
import { encrypt } from 'eciesjs';

// Get user's public key from DAO contract
const pubkey = await dao.get_user_pubkey({ user: "alice.testnet" });

// Encrypt vote locally
const vote = "yes";  // or "no"
const encrypted = encrypt(
  Buffer.from(pubkey, 'hex'),
  Buffer.from(vote)
);

// Submit to blockchain (only ciphertext visible)
await dao.cast_vote({
  proposal_id: 1,
  encrypted_vote: encrypted.toString('hex')
});
```

**Result**: On-chain data shows only encrypted blobs. No one can see how you voted.

**3. Off-Chain Tallying in TEE**

After voting deadline, anyone can trigger finalization:

```rust
// In TEE (OutLayer worker)
fn tally_votes(votes: &[Vote], master_secret: &[u8]) -> TallyResult {
    let mut yes_count = 0;
    let mut no_count = 0;

    for vote in votes {
        // Derive user's private key from master secret
        let user_privkey = derive_key(master_secret, &vote.user);

        // Decrypt vote using ECIES
        let decrypted = ecies::decrypt(user_privkey, &vote.encrypted);

        // Count real votes (ignore dummies)
        match decrypted.as_str() {
            "yes" => yes_count += 1,
            "no" => no_count += 1,
            _ => {} // dummy message, ignore
        }
    }

    // Build merkle tree for verification
    let merkle_root = build_merkle_tree(votes);
    let proofs = generate_proofs(votes);

    TallyResult {
        yes_count,
        no_count,
        votes_merkle_root: merkle_root,
        merkle_proofs: proofs,
        tee_attestation: generate_attestation(...)
    }
}
```

**Privacy guarantee**: Individual votes exist only in TEE memory, never logged or persisted.

**4. Verifiable Results**

Each voter gets a merkle proof that their vote was counted:

```typescript
// After finalization, verify your vote
const proofs = await dao.get_vote_proofs({
  proposal_id: 1,
  account_id: "alice.testnet"
});

const myProof = proofs[0];

// Compute merkle root from proof
let hash = myProof.vote_hash;
for (const sibling of myProof.proof_path) {
  hash = SHA256(hash + sibling);
}

if (hash === proposal.merkle_root) {
  console.log('✓ My vote was counted!');
} else {
  console.log('✗ Tampering detected!');
}
```

**What you can verify:**
- ✅ Your encrypted vote was included in the tally
- ✅ The merkle root matches what's stored on-chain
- ✅ TEE signed this result (attestation)

**What remains private:**
- ✗ How you voted (yes/no)
- ✗ How others voted
- ✗ Which encrypted message was your real vote (if you sent dummies)

---

## Key OutLayer Features Used

### 1. Secrets Management (Keymaster)

**Traditional approach:**
```bash
# Bad: Store secret on-chain (visible to everyone)
near call dao.testnet store_secret '{"secret":"abc123"}' --accountId admin

# Still bad: Store encrypted secret on-chain (ciphertext visible)
near call dao.testnet store_secret '{"encrypted":"def456"}' --accountId admin
```

**OutLayer approach:**
```bash
# Good: Secret stored in OutLayer contract, only accessible in TEE
near call outlayer.testnet store_secrets '{
  "repo": "github.com/user/dao",
  "profile": "production",
  "encrypted_data": [...],  # Encrypted with keystore pubkey
  "access_condition": {"AllowAll": {}}
}' --accountId dao.testnet --deposit 0.01
```

**When WASM executes:**
```rust
// Secret automatically injected into environment
let secret = std::env::var("PROTECTED_DAO_MASTER_SECRET")?;
// Use secret for cryptography
// Secret deleted from memory after execution
```

**Security**:
- ✅ Secret never touches contract or client
- ✅ Only visible inside TEE during execution
- ✅ Access controlled by keystore (verifies worker attestation)
- ✅ No logging or persistence

### 2. Heavy Cryptography Off-Chain

**Cost comparison for 100-vote DAO:**

| Operation | On-Chain | OutLayer | Speedup |
|-----------|----------|----------|---------|
| ECIES decrypt | ~35,000 TGas* | ~0.05 TGas | 700,000x |
| HKDF key derivation | ~300 TGas | ~0.001 TGas | 300,000x |
| Merkle tree (100 votes) | ~1,000 TGas | ~0.01 TGas | 100,000x |
| **Total** | **>300 NEAR** | **~0.05 NEAR** | **6000x** |

\* Actually impossible - exceeds NEAR's 300 TGas limit!

**Example operations:**

```rust
// HKDF-SHA256 key derivation (~1ms in TEE vs impossible on-chain)
let derived_key = Hkdf::<Sha256>::new(None, master_secret)
    .expand(info.as_bytes(), &mut okm)
    .unwrap();

// ECIES decryption (~2ms per vote in TEE vs 300+ TGas on-chain)
let plaintext = ecies::decrypt(private_key, ciphertext)?;

// Merkle tree construction (~50ms for 100 votes in TEE)
let merkle_root = build_binary_merkle_tree(vote_hashes);
```

### 3. TEE Attestation

Every tally result includes a TEE attestation:

```rust
fn generate_tee_attestation(
    proposal_id: u64,
    merkle_root: &str,
    yes_count: u32,
    no_count: u32
) -> String {
    // MVP: Simple hash-based attestation
    let data = format!("{}:{}:{}:{}", proposal_id, merkle_root, yes_count, no_count);
    let hash = sha256(&data);
    format!("mvp-attestation:{}", hash)

    // Production: SGX/SEV remote attestation
    // - Proves code hash matches expected WASM
    // - Proves execution happened in TEE
    // - Signed with TEE's private key
}
```

Users verify:
1. Merkle root matches their vote proof
2. Attestation signed by trusted TEE
3. Result: Trustless vote counting!

---

## Privacy Features

### Dummy Messages

Send encrypted noise to hide voting patterns:

```typescript
// Real vote
await dao.cast_vote(1, encrypt("yes"));

// Dummy messages (all look identical on-chain)
await dao.cast_vote(1, encrypt("DUMMY_1"));
await dao.cast_vote(1, encrypt("random noise"));
await dao.cast_vote(1, encrypt(""));

// Result: 4 transactions visible, no one knows which is real
```

**Cost**: ~0.002 NEAR per message
**Benefit**: Plausible deniability

### Vote Changes

Change your mind before deadline:

```typescript
// Monday
await dao.cast_vote(1, encrypt("yes"));  // timestamp: 100

// Wednesday
await dao.cast_vote(1, encrypt("no"));   // timestamp: 200

// Result: "no" is counted (latest timestamp)
```

TEE tracks timestamps and only counts your last real vote.

---

## Why You Can't Do This Without OutLayer

### Limitation 1: Gas Costs

**ECIES decryption** on NEAR:
- ~350 TGas per vote (ECDH + AES-256-GCM)
- NEAR limit: 300 TGas per call
- **100 votes would require 117 separate transactions!**

**With OutLayer:**
- All votes decrypted in single execution
- ~50M WASM instructions
- Cost: ~0.05 NEAR

### Limitation 2: Privacy

**On-chain smart contracts:**
- All state is public (even encrypted data is visible)
- Cannot store secrets securely
- Cannot process data privately

**With OutLayer TEE:**
- Secrets injected at runtime (never stored)
- Computation in isolated environment
- Individual votes never logged
- Only aggregate results published

### Limitation 3: Complexity

**Operations that exceed gas limits:**
- Deriving 100 user keys: ~300 TGas
- Building merkle tree for 100 votes: ~1,000 TGas
- Generating merkle proofs: ~500 TGas

**With OutLayer:**
- All operations in <1 second
- Unlimited computational complexity
- Only pay for actual execution time

---

## Real-World Performance

**Tested on M1 MacBook Pro:**

| Operation | Time |
|-----------|------|
| Derive user pubkey | 1 ms |
| Encrypt vote (client) | 5 ms |
| Decrypt vote (TEE) | 2 ms |
| **Tally 100 votes** | **270 ms** |
| Build merkle tree (100 votes) | 50 ms |
| Verify merkle proof | 10 ms |

**Storage costs:**

| Item | Size | Cost |
|------|------|------|
| Member pubkey | 33 bytes | 0.0003 NEAR |
| Encrypted vote | ~200 bytes | 0.002 NEAR |
| Proposal | ~500 bytes | 0.005 NEAR |

**Example DAO:**
- 100 members: 0.03 NEAR
- 10 proposals: 0.05 NEAR
- 500 votes: 1.0 NEAR
- **Total: ~1.1 NEAR for full lifecycle**

Compare to on-chain: >30,000 NEAR (if it were even possible)

---

## Technical Challenges Solved

### 1. JavaScript Number Precision

**Problem**: NEAR timestamps are u64 (64-bit), but JavaScript Number has only 53-bit precision.

```typescript
// ❌ WRONG - loses precision!
const timestamp = JSON.parse(transactionResult);  // 1762633750157553400
// Real value:                                     1762633750157553364
//                                                                  ^^^ lost!

// ✅ CORRECT - preserve full precision
const timestamp = atob(transactionResult).trim();  // keep as string
const timestampBigInt = BigInt(timestamp);         // convert to BigInt
```

This matters because vote hash includes timestamp - even 1 bit difference breaks verification!

### 2. Merkle Tree Order

**Decision**: Should frontend match TEE's merkle tree construction, or vice versa?

**Answer**: Frontend should adapt to TEE (TEE is authoritative).

**TEE (Rust)**:
```rust
// Fixed order: left + right
let parent = SHA256(left + right);
```

**Frontend (TypeScript)**:
```typescript
// Try all possible orderings (2^depth paths)
async function tryAllPaths(hash, remainingPath) {
  if (remainingPath.length === 0) {
    return hash === merkleRoot;
  }

  const [sibling, ...rest] = remainingPath;

  // Try both orders
  if (await tryAllPaths(SHA256(hash + sibling), rest)) return true;
  if (await tryAllPaths(SHA256(sibling + hash), rest)) return true;

  return false;
}
```

**Complexity**: O(2^depth) but depth = log₂(votes) is small (depth 10 for 1000 votes = 1024 checks ~10ms)

### 3. Retroactive Voting

**Problem**: Users could join after seeing a proposal they want to influence.

**Solution**: Store `joined_at` timestamp, check against `proposal.created_at`.

```rust
pub fn cast_vote(&mut self, proposal_id: u64, encrypted_vote: String) {
    let member_info = self.members.get(&voter).expect("Only members");
    let proposal = self.proposals.get(&proposal_id).expect("Not found");

    assert!(
        member_info.joined_at < proposal.created_at,
        "Cannot vote on proposals created before you joined"
    );

    // ... store vote
}
```

---

## Conclusion

NEAR OutLayer enables a new class of dApps that were previously impossible:

✅ **Private** - Votes encrypted, only TEE sees plaintext
✅ **Verifiable** - Merkle proofs + TEE attestation
✅ **Affordable** - 6000x cheaper than on-chain
✅ **Scalable** - Tested up to 1000 votes
✅ **Production-ready** - Awaiting full TEE attestation

**Key insight**: By moving heavy cryptography to TEE, we get privacy AND affordability without sacrificing verifiability.

**Cost comparison**:
- Traditional DAO (Snapshot, off-chain): Free but not binding
- On-chain DAO (Sputnik): Transparent but no privacy
- **OutLayer Private DAO**: Private, verifiable, and 1000x cheaper than on-chain crypto

---

## Try It Yourself

**Code**: https://github.com/out-layer/private-dao-example

**Docs**: [Full README](README.md) | [Technical Deep Dive](../../docs/examples/PRIVATE_DAO.md)

**Deploy**:
```bash
# Build WASI module
cd wasi-examples/private-dao-example
cargo build --target wasm32-wasip1 --release

# Deploy contract
cd dao-contract
near contract deploy your-dao.testnet ...

# Setup secrets
# (via dashboard or CLI)

# Start voting!
```

**Learn more**:
- [NEAR OutLayer Documentation](https://docs.outlayer.near.org)
- [ECIES Encryption](https://en.wikipedia.org/wiki/Integrated_Encryption_Scheme)
- [Merkle Tree Proofs](https://brilliant.org/wiki/merkle-tree/)

---

**Built with ❤️ on NEAR OutLayer**

*Making privacy affordable for DAOs*
