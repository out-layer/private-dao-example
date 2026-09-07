# NEAR OutLayer - Anonymous DAO Voting Architecture

**Version:** 1.0 MVP
**Date:** 2025-11-07
**Status:** Design Phase

---

## 1. Executive Summary

### 1.1 Purpose

A **cryptographically secure anonymous voting system** for NEAR DAOs that showcases OutLayer's capability to execute heavy zero-knowledge proof generation off-chain while maintaining on-chain verifiability.

### 1.2 Core Properties

| Property | Guarantee | Mechanism |
|----------|-----------|-----------|
| **Vote Privacy** | Individual votes hidden | Per-user encryption keys |
| **Membership Proof** | Only DAO members vote | ZK proof of membership (nullifier-based) |
| **No Double Voting** | One vote per member per proposal | Nullifier uniqueness (cryptographic) |
| **Verifiable Tallying** | Correct count guaranteed | ZK proof of correct decryption + tallying |
| **Censorship Resistance** | Cannot block specific votes | On-chain commitment, off-chain computation |
| **Result Integrity** | Cannot fake results | TEE attestation + ZK proof verification |

### 1.3 What Makes This Real (Not Bullshit)

1. **ZK Proofs are actual cryptographic proofs**
   - Groth16/PLONK circuits (not simulated)
   - On-chain verification (BN254 pairing)
   - Mathematically impossible to forge

2. **OutLayer enables ZK at scale**
   - Proof generation: 10-30 seconds per vote (too heavy for client)
   - OutLayer executes in parallel for N voters
   - Result: Sub-minute finalization for 100+ voters

3. **Privacy is cryptographic, not assumed**
   - Each user has unique encryption key (derived from master secret)
   - ECIES with random nonce per message
   - TEE ensures master secret never leaks

4. **Verifiability is on-chain**
   - Anyone can verify ZK proofs
   - Contract validates before accepting results
   - No "trust the worker" - math proves correctness

---

## 2. System Architecture

### 2.1 Components Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                         User (Web Client)                        │
│  - Generate voting identity (secret, commitment)                 │
│  - Create ZK proof of membership (locally, heavy!)               │
│  - Encrypt vote with user-specific key                           │
│  - Submit: {encrypted_vote, zk_proof, nullifier}                 │
└────────────────────────┬────────────────────────────────────────┘
                         │
                         │ On-chain transaction
                         ↓
┌─────────────────────────────────────────────────────────────────┐
│                    DAO Contract (NEAR)                           │
│                                                                   │
│  Storage:                                                         │
│  ├─ Members Merkle Tree (commitments)                            │
│  ├─ Per-user encryption pubkeys                                  │
│  ├─ Encrypted votes: {user, ciphertext, nonce, timestamp}        │
│  ├─ Used nullifiers (prevent double voting)                      │
│  └─ Proposals: {id, quorum, deadline, result}                    │
│                                                                   │
│  Functions:                                                       │
│  ├─ join() - Add commitment to Merkle tree                       │
│  ├─ create_proposal() - Create vote with quorum                  │
│  ├─ cast_vote() - Verify ZK proof + store encrypted vote         │
│  └─ finalize() - Request OutLayer tallying                       │
└────────────────────────┬────────────────────────────────────────┘
                         │
                         │ After voting deadline
                         ↓
┌─────────────────────────────────────────────────────────────────┐
│                    OutLayer Worker (TEE)                         │
│                                                                   │
│  Task 1: Decrypt votes                                           │
│  ├─ Fetch master_secret from Keymaster (TEE-to-TEE)             │
│  ├─ Re-derive each user's privkey                                │
│  ├─ Decrypt all encrypted votes                                  │
│  └─ Filter: count only "yes"/"no", ignore dummy                  │
│                                                                   │
│  Task 2: Generate ZK proof of correct tallying (Phase 2)         │
│  ├─ Public inputs: votes_merkle_root, result_commitment          │
│  ├─ Private inputs: decrypted_votes, master_secret               │
│  ├─ Circuit proves: "I decrypted correctly and counted fairly"   │
│  └─ Output: Groth16 proof (~200 bytes)                           │
│                                                                   │
│  Task 3: Return result + proof                                   │
│  └─ {passed: bool, vote_count: u32, tee_attestation}            │
└────────────────────────┬────────────────────────────────────────┘
                         │
                         │ Callback
                         ↓
┌─────────────────────────────────────────────────────────────────┐
│                DAO Contract - Finalization                       │
│                                                                   │
│  1. Verify TEE attestation                                       │
│  2. Check votes_merkle_root matches stored data                  │
│  3. If valid: Accept result                                      │
│  4. Publish: {proposal_id, passed: bool, vote_count}             │
│  5. Store proof for public verification                          │
└─────────────────────────────────────────────────────────────────┘
```

### 2.2 MVP vs Phase 2

**MVP (Phase 1):** Focus on OutLayer showcase
- ✅ Encrypted voting (ECIES)
- ✅ TEE tallying via OutLayer
- ✅ Dummy messages for privacy
- ✅ Storage deposit mechanism
- ❌ NO ZK proofs yet (TEE attestation only)

**Phase 2:** Add full ZK proofs
- ✅ ZK proof of membership (client-side)
- ✅ ZK proof of tallying (OutLayer)
- ✅ On-chain verification (Groth16)

---

## 3. Cryptographic Primitives

### 3.1 Key Derivation (Deterministic)

**Purpose:** Each user has unique encryption key without storing N private keys.

```
Inputs:
  - master_secret: 256-bit random (stored in Keymaster TEE)
  - dao_account: "dao.near"
  - user_account: "alice.near"

Derivation (HKDF-SHA256):
  info = "user:" || dao_account || ":" || user_account
  user_privkey = HKDF-SHA256(master_secret, salt=None, info, length=32)
  user_pubkey = secp256k1_derive_pubkey(user_privkey)

Properties:
  ✅ Deterministic: Same inputs → same key
  ✅ Isolated: user_privkey_alice ≠ user_privkey_bob
  ✅ One-way: Cannot derive master_secret from pubkey
  ✅ Efficient: O(1) to derive any user's key on-demand
```

### 3.2 Vote Encryption (ECIES)

**ECIES (Elliptic Curve Integrated Encryption Scheme):**

```
Encryption (client-side):
  1. Generate ephemeral keypair: (eph_priv, eph_pub)
  2. Compute shared secret: shared = ECDH(eph_priv, user_pubkey)
  3. Derive keys: (enc_key, mac_key) = HKDF(shared, nonce)
  4. Encrypt: ciphertext = AES-256-GCM(vote, enc_key)
  5. MAC: tag = HMAC-SHA256(ciphertext, mac_key)
  6. Output: (eph_pub, nonce, ciphertext, tag)

Decryption (TEE Worker):
  1. Derive user_privkey from master_secret
  2. Compute shared secret: shared = ECDH(user_privkey, eph_pub)
  3. Derive keys: (enc_key, mac_key) = HKDF(shared, nonce)
  4. Verify MAC: tag == HMAC-SHA256(ciphertext, mac_key) ✅
  5. Decrypt: vote = AES-256-GCM-decrypt(ciphertext, enc_key)

Properties:
  ✅ Semantic security (IND-CCA2)
  ✅ Random nonce → different ciphertexts for same vote
  ✅ Authenticated encryption (cannot tamper)
  ✅ Forward secrecy (ephemeral keys)
```

**Nonce:** 128-bit random (client-generated)
- Prevents ciphertext correlation
- Each message unique even with same content

### 3.3 Dummy Messages (Privacy Enhancement)

**Purpose:** Add noise to hide voting patterns

```
User can submit multiple messages:
  - Real vote: "yes" or "no"
  - Dummy: any other content (empty, "DUMMY", random)

Worker filters during tallying:
  - Count only: "yes" and "no"
  - Ignore: everything else

Result:
  - Observer sees N transactions per user
  - Cannot tell which are real votes
  - Plausible deniability for participation patterns
```

---

## 4. Storage & Economics

### 4.1 Storage Costs

**Per-User Storage:**
```
Join DAO:
  - Commitment (32 bytes): 0.0003 NEAR
  - Pubkey (33 bytes): 0.0003 NEAR
  Total: ~0.0006 NEAR per member

Per Vote:
  - Encrypted message (~200 bytes): 0.002 NEAR
  - User can send multiple (real + dummy)

Example: 1 real + 2 dummy = 3 * 0.002 = 0.006 NEAR
```

**Storage NOT Refunded:**
- Data must persist for verification
- Historical votes needed for auditing
- Anyone can verify integrity

### 4.2 Quorum Types

```rust
pub enum QuorumType {
    /// Need absolute number of votes
    Absolute { required_votes: u32 },

    /// Need percentage of members at proposal creation
    Percentage { percent: u8 },  // 0-100

    /// Need percentage of actual voters to vote yes
    PercentageOfVoters { percent: u8 },
}
```

**Examples:**
```
Proposal with 100 members:

Type 1: Absolute(10)
  - Need 10 votes minimum
  - yes must be > no

Type 2: Percentage(50)
  - Need 50% of 100 = 50 votes
  - yes must be > no

Type 3: PercentageOfVoters(60)
  - Need 60% of voters to vote yes
  - If 40 voted: need 24 yes votes
```

---

## 5. Security Model

### 5.1 Trust Assumptions

**Trusted:**
- ✅ TEE hardware (Intel SGX / AMD SEV)
- ✅ Cryptographic primitives (secp256k1, AES-256)
- ✅ NEAR blockchain consensus

**Not Trusted:**
- ❌ OutLayer Worker (verified via TEE attestation)
- ❌ Keymaster (master_secret encrypted, TEE-only)
- ❌ Frontend (open-source, user verifiable)

### 5.2 Attack Mitigations

**Ciphertext Correlation:**
- ✅ Prevented by random nonces
- ✅ ECIES ephemeral keys
- ✅ Each message cryptographically unique

**Double Voting:**
- ✅ Phase 1: Last vote counts (contract logic)
- ✅ Phase 2: Nullifier uniqueness (ZK proof)

**Timing Analysis:**
- ⚠️ Blockchain timestamps public
- ✅ Dummy messages add noise
- ✅ Users can delay submission

**Worker Malicious Behavior:**
- ✅ TEE attestation verifies code
- ✅ Master secret never leaves TEE
- ✅ Phase 2: ZK proof prevents fake results

---

## 6. Implementation Phases

### Phase 1: MVP (4 weeks)

**Goal:** Showcase OutLayer for encryption operations

**Features:**
- DAO contract (join, create, vote, finalize)
- Crypto WASI module (key derivation, ECIES)
- Tally WASI module (decrypt + count)
- Frontend (basic voting UI)
- TEE attestation only (no ZK proofs)

**Deliverables:**
1. Working demo video
2. Documentation (this file + README)
3. Open-source code (GitHub)

### Phase 2: ZK Proofs (6 weeks)

**Added Features:**
- Merkle tree for members
- ZK proof of membership (Semaphore circuit)
- ZK proof of tallying (custom circuit)
- Groth16 verifier contract
- Public verification tools

**Showcase:** OutLayer generates ZK proofs 100x faster than client

---

## 7. Privacy Guarantees

### 7.1 What is Private?

| Information | Privacy Level | Mechanism |
|-------------|---------------|-----------|
| Individual vote | ❌ Hidden | ECIES encryption |
| Who voted | ✅ Public | Transaction visible |
| When voted | ✅ Public | Block timestamp |
| Message count | ✅ Public | Can include dummy |
| Final result | ✅ Public | passed + vote_count |
| Yes/No counts | ❌ Hidden (optional) | Not published |

### 7.2 Privacy vs Transparency

**MVP Approach (Balanced):**
- Publish: `passed: bool, vote_count: u32`
- Hide: Exact yes/no breakdown
- Store: All encrypted votes (for verification)

**Users can verify:**
- Download encrypted votes from chain
- Verify integrity (merkle root)
- Optional: Decrypt own vote locally

---

## 8. Future Enhancements

### Short-term (Phase 2)
- ZK proof of membership (Semaphore)
- ZK proof of tallying (custom circuit)
- On-chain verification (Groth16)

### Long-term
- Weighted voting (token-based)
- Quadratic voting
- Delegation (vote by proxy)
- Multi-party computation (no single master secret)
- Post-quantum cryptography

---

## 9. References

**Cryptographic Primitives:**
- ECIES: [SEC 1 v2.0](https://www.secg.org/sec1-v2.pdf)
- HKDF: [RFC 5869](https://tools.ietf.org/html/rfc5869)
- secp256k1: [Bitcoin's curve](https://en.bitcoin.it/wiki/Secp256k1)

**Zero-Knowledge Proofs:**
- Semaphore: [semaphore.pse.dev](https://semaphore.pse.dev)
- Groth16: [Original paper](https://eprint.iacr.org/2016/260)
- PLONK: [Original paper](https://eprint.iacr.org/2019/953)

**OutLayer Platform:**
- Documentation: [OutLayer docs](https://github.com/near-offshore/docs)
- WASI Tutorial: [WASI_TUTORIAL.md](https://github.com/out-layer/outlayer/blob/main/wasi-examples/WASI_TUTORIAL.md)

---

**END OF ARCHITECTURE DOCUMENT**

This system provides cryptographically secure anonymous voting with verifiable tallying, showcasing OutLayer's capability to execute heavy cryptographic operations off-chain while maintaining on-chain trust.
