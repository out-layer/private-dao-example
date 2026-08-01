# Private DAO - Issues and TODOs

## 🔴 Critical Issues

### 1. ✅ ECIES Encryption - COMPLETED (2025-11-07)
**Status**: ✅ **RESOLVED**
**Implementation**:
- ✅ Full ECIES implementation using `ecies` crate with pure Rust backend
- ✅ Generates actual secp256k1 keypairs from deterministic HKDF seed
- ✅ Returns real compressed public keys (33 bytes: 0x02/0x03 + X coordinate)
- ✅ Uses libsecp256k1 for key generation (WASI-compatible)
- ✅ All 7 unit tests passing (deterministic keys, encryption/decryption)
- ✅ WASM binary compiles successfully (280 KB)

**Crypto Stack**:
- **HKDF-SHA256**: Deterministic key derivation from master secret + DAO + user
- **secp256k1**: Elliptic curve for keypairs
- **ECIES**: Integrated encryption scheme (secp256k1 + AES-256-GCM + HMAC)
- **Nonce handling**: Included in ciphertext (no separate nonce parameter)

**Remaining work**:
- [ ] Update frontend encryption in VoteOnProposal.tsx (use ECIES library)
- [ ] Test end-to-end encryption flow (client → contract → TEE)

---

## 🟡 Important TODOs

### 2. ✅ Frontend Implementation - COMPLETED (2025-11-07)
**Status**: ✅ **FULLY IMPLEMENTED**
**Completed**:
- ✅ React app created with TypeScript
- ✅ @near-wallet-selector integrated (MyNearWallet)
- ✅ Wallet connection implemented
- ✅ All components created:
  - ✅ JoinDAO.tsx - Join DAO and member management
  - ✅ CreateProposal.tsx - Create governance proposals
  - ✅ ProposalList.tsx - View all proposals
  - ✅ VoteOnProposal.tsx - Cast encrypted votes with **REAL ECIES**
- ✅ Client-side ECIES encryption with `eciesjs` library
- ✅ Full UI with responsive design

**Frontend encryption stack**:
- **eciesjs**: JavaScript ECIES implementation (secp256k1)
- **TextEncoder**: Convert vote strings to bytes
- **Buffer**: Convert ciphertext to hex for contract storage
- **Compatible**: Matches Rust backend (ecies crate)

**Remaining work**:
- [ ] Add vote verification UI (show vote proof)
- [ ] Add error handling for network failures

### 3. Contract Deployment Scripts Missing
**Status**: Not created
**Required**:
- [ ] Create deploy.sh script
- [ ] Document initialization parameters
- [ ] Create example keymaster secret setup
- [ ] Test deployment to testnet

### 4. ✅ Client-Side Encryption Library - COMPLETED (2025-11-07)
**Status**: ✅ **RESOLVED**
**Implementation**:
- ✅ Integrated `eciesjs` npm package (v0.4+)
- ✅ Full ECIES encryption in VoteOnProposal.tsx
- ✅ Matches Rust backend (secp256k1 + AES-256-GCM)
- ✅ No nonce parameter needed (included in ciphertext)
- ✅ Hex conversion with Buffer.toString('hex')

### 5. Master Secret Management
**Status**: Not documented
**Problem**: How to generate and store master secret in keymaster?
**Required**:
- [ ] Document secret generation process
- [ ] Create script to upload to keymaster
- [ ] Document keymaster access conditions
- [ ] Test secret retrieval from OutLayer worker

---

## 🟢 Minor Issues

### 6. Contract Warnings
**Issue**: 2 warnings during compilation
```
warning: trait `OutLayer` is never used
warning: trait `ExtSelf` is never used
```
**Impact**: Low (just warnings, code works)
**Fix**: Add `#[allow(dead_code)]` to ext_contract traits

### 7. No Gas Estimation
**Issue**: Fixed gas values in contract
**Impact**: May fail if OutLayer gas requirements change
**Fix**: Add configurable gas limits (owner can update)

### 8. No Event Logging
**Issue**: Contract doesn't emit NEP-297 events
**Impact**: Hard to index/track on frontend
**Fix**: Add events for:
- [ ] Member joined
- [ ] Proposal created
- [ ] Vote cast
- [ ] Proposal finalized

### 9. No Pagination for Members List
**Issue**: Contract has no `get_members()` view method
**Impact**: Can't display member list on frontend
**Fix**: Add view method with pagination

### 10. Storage Deposits Not Refundable
**Issue**: User pays 0.002 NEAR per vote, never refunded
**Impact**: Expensive for active voters
**Discussion**: Is this intentional? (historical data vs cost)

---

## 🔵 Future Enhancements (Phase 2)

### 11. Zero-Knowledge Proofs
**Status**: Documented in ARCHITECTURE.md, not implemented
**Components needed**:
- [ ] Semaphore-style membership proof circuit
- [ ] Tallying correctness proof circuit
- [ ] On-chain verifier contract
- [ ] Client-side proof generation library

### 12. Real TEE Attestation
**Status**: MVP uses placeholder hash
**Required for production**:
- [ ] SGX remote attestation
- [ ] Or SEV-SNP attestation
- [ ] Attestation verification on contract
- [ ] Public attestation verification service

### 13. Vote Delegation
**Status**: Not designed
**Feature**: Allow members to delegate voting power
**Requires**: New contract methods and TEE tallying logic

### 14. Proposal Execution
**Status**: Not implemented
**Feature**: Auto-execute passed proposals (e.g., transfer funds)
**Requires**: Proposal action framework

---

## 📝 Documentation Gaps

### 15. End-to-End Tutorial Missing
**Required**:
- [ ] Step-by-step guide: Deploy → Join → Vote → Verify
- [ ] Screenshots/video walkthrough
- [ ] Testnet deployment example
- [ ] Troubleshooting guide

### 16. API Reference Incomplete
**Required**:
- [ ] Document all contract methods with examples
- [ ] Document WASI module input/output formats
- [ ] Add sequence diagrams for flows
- [ ] Document error codes and messages

### 17. Security Audit Needed
**Status**: Not audited
**Critical for mainnet**:
- [ ] Smart contract audit
- [ ] WASI module audit
- [ ] Cryptography review
- [ ] Gas optimization review

---

## 🐛 Known Bugs

### 18. No Duplicate Vote Prevention at Contract Level
**Issue**: User can submit multiple votes in same block
**Impact**: Wastes storage, clutters vote list
**Current behavior**: Last vote wins (handled in TEE)
**Better approach**: Contract checks for existing vote and updates

### 19. No Proposal Cancellation
**Issue**: Creator can't cancel/delete proposal
**Impact**: Spam proposals clutter UI
**Fix**: Add `cancel_proposal()` method (creator only, before votes)

### 20. Deadline Validation Not Strict
**Issue**: Deadline can be 1 nanosecond in future
**Impact**: Proposal can expire immediately
**Fix**: Add minimum voting period (e.g., 1 hour)

---

## ⚡ Performance Concerns

### 21. Vote Storage Grows Unbounded
**Issue**: All votes stored on-chain forever
**Impact**: High storage costs for active DAOs
**Solution options**:
- Archive old votes to IPFS after finalization
- Implement vote pruning after verification period
- Use merkle root only (store votes off-chain)

### 22. Get All Votes Inefficient
**Issue**: `get_votes()` returns entire vector
**Impact**: RPC limits for proposals with many votes
**Fix**: Add pagination or return vote count only

### 23. No Vote Batching
**Issue**: Each vote is separate transaction
**Impact**: High transaction fees for users
**Enhancement**: Allow batch voting on multiple proposals

---

## 🎨 UX Improvements Needed

### 24. No Vote Preview
**Issue**: User doesn't see what they're voting on before encryption
**Fix**: Add confirmation modal with proposal details

### 25. No Voting Power Display
**Issue**: Members don't know if their vote counts (quorum)
**Fix**: Show current vote count and quorum progress (without revealing results)

### 26. No Proposal Search/Filter
**Issue**: Hard to find proposals in large DAOs
**Fix**: Add search by title, filter by status, sort by deadline

### 27. No Notifications
**Issue**: Members miss new proposals and deadlines
**Fix**: Add on-chain notification events, integrate with wallet notifications

---

## 🔐 Security Considerations

### 28. No Rate Limiting
**Issue**: User can spam join_dao() or cast_vote() calls
**Impact**: DoS potential, storage bloat
**Fix**: Add rate limiting or cooldown periods

### 29. No Membership Revocation
**Issue**: Owner can't remove malicious members
**Impact**: Compromised accounts can vote forever
**Fix**: Add `remove_member()` method (owner only)

### 30. Master Secret Rotation Not Supported
**Issue**: If master secret compromised, can't rotate
**Impact**: All user keys compromised forever
**Fix**: Design key rotation mechanism (complex)

---

## 📊 Testing Gaps

### 31. No Integration Tests
**Status**: Only unit tests in WASI module
**Required**:
- [ ] Contract unit tests
- [ ] Contract integration tests (deploy → join → vote → finalize)
- [ ] E2E tests with real OutLayer deployment
- [ ] Load testing (100+ members, 1000+ votes)

### 32. No Frontend Tests
**Status**: Frontend not created yet
**Required**:
- [ ] Component tests (Jest + React Testing Library)
- [ ] E2E tests (Playwright/Cypress)
- [ ] Wallet integration tests

---

## 🌐 Deployment Checklist

### Before Testnet:
- [ ] Fix critical issue #1 (ECIES encryption)
- [ ] Create frontend (issue #2)
- [ ] Create deployment scripts (issue #3)
- [ ] Setup keymaster secret (issue #5)
- [ ] Write end-to-end tutorial (issue #15)
- [ ] Add basic events (issue #8)

### Before Mainnet:
- [ ] Security audit (issue #17)
- [ ] Real TEE attestation (issue #12)
- [ ] Fix all known bugs (#18-20)
- [ ] Optimize storage (issue #21)
- [ ] Add rate limiting (issue #28)
- [ ] Add membership revocation (issue #29)
- [ ] Complete testing (issues #31-32)
- [ ] ZK proofs (issue #11) - optional but recommended

---

## 💡 Open Questions

1. ~~**Encryption approach**: Should we fix ECIES now or ship MVP with symmetric encryption workaround?~~ ✅ **RESOLVED** - Full ECIES implemented
2. **Storage costs**: Should votes be refundable or permanent? What about archived proposals?
3. **Quorum calculation**: Should abstentions count toward quorum? Currently they don't.
4. **Vote privacy level**: Should we hide vote counts until finalization? (Currently visible via `get_vote_count`)
5. **DAO treasury**: Should DAO hold funds? Where would execution payments come from?
6. **Membership criteria**: Should we support token-gated membership (NFT holders, FT holders)?
7. **Proposal types**: Should we support different proposal types (governance, treasury, member addition)?

---

**Last Updated**: 2025-11-07
**Status**: MVP in development, needs frontend + critical fixes before testnet
