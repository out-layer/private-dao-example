# Private DAO - Deployment Guide

## 🚀 Quick Start (Testnet)

### Prerequisites
- Node.js 16+ and npm
- Rust toolchain with wasm32-unknown-unknown target
- NEAR CLI (`npm install -g near-cli`)
- NEAR testnet account

---

## Step 1: Build WASI Module

```bash
cd private-dao-example

# Build WASI module
./build.sh

# Verify build
ls -lh target/wasm32-wasip1/release/private-dao-example.wasm
```

**Expected output**: 167K WASM file

---

## Step 2: Build DAO Contract

```bash
cd dao-contract

# Build contract
./build.sh

# Verify build
ls -lh res/private_dao_contract.wasm
```

**Expected output**: 317K WASM file

---

## Step 3: Deploy Contract to Testnet

```bash
# Login to NEAR CLI
near login

# Create subaccount for DAO
near create-account my-dao.YOURACCOUNT.testnet --masterAccount YOURACCOUNT.testnet --initialBalance 10

# Deploy contract
near deploy my-dao.YOURACCOUNT.testnet \
  --wasmFile dao-contract/res/private_dao_contract.wasm \
  --initFunction new \
  --initArgs '{
    "name": "My Private DAO",
    "membership_mode": "Public",
    "owner": "YOURACCOUNT.testnet"
  }'
```

**Replace `YOURACCOUNT.testnet` with your actual account!**

---

## Step 4: Setup Master Secret in Keymaster

### Generate Master Secret

```bash
# Generate random 32-byte hex secret
openssl rand -hex 32
```

**Example output**: `a1b2c3d4e5f6...` (64 hex characters)

### Store in Keymaster (TODO - requires keymaster-worker setup)

```bash
# This step requires keymaster-worker to be running
# For MVP testing, you can skip this and use placeholder secrets

# TODO: Document keymaster secret upload process
# near call keymaster.testnet store_secret '{...}'
```

**For MVP**: OutLayer will use test secrets temporarily

---

## Step 5: Test Contract Manually

### Join DAO (Public Mode)

```bash
near call my-dao.YOURACCOUNT.testnet join_dao \
  --accountId user1.testnet \
  --deposit 0.012 \
  --gas 100000000000000
```

**Wait ~30 seconds for OutLayer to generate encryption key**

### Check Membership

```bash
near view my-dao.YOURACCOUNT.testnet is_member \
  '{"account_id": "user1.testnet"}'
```

**Expected**: `true`

### Check Public Key (after OutLayer callback)

```bash
near view my-dao.YOURACCOUNT.testnet get_user_pubkey \
  '{"account_id": "user1.testnet"}'
```

**Expected**: Hex string (64 characters)

### Create Proposal

```bash
# Calculate deadline (7 days from now, in nanoseconds)
DEADLINE=$(( ($(date +%s) + 604800) * 1000000000 ))

near call my-dao.YOURACCOUNT.testnet create_proposal \
  '{
    "title": "Increase membership limit",
    "description": "Should we allow more members to join?",
    "quorum": {"Absolute": {"min_votes": 3}},
    "deadline": "'$DEADLINE'"
  }' \
  --accountId user1.testnet \
  --deposit 0.001 \
  --gas 50000000000000
```

### Cast Vote

```bash
# TODO: Implement proper encryption on client side
# For MVP, using placeholder encrypted vote

near call my-dao.YOURACCOUNT.testnet cast_vote \
  '{
    "proposal_id": 1,
    "encrypted_vote": "796573",
    "nonce": "0123456789abcdef0123456789abcdef"
  }' \
  --accountId user1.testnet \
  --deposit 0.002 \
  --gas 50000000000000
```

### Finalize Proposal (after deadline)

```bash
near call my-dao.YOURACCOUNT.testnet finalize_proposal \
  '{"proposal_id": 1}' \
  --accountId user1.testnet \
  --deposit 0.01 \
  --gas 150000000000000
```

**Wait ~30 seconds for OutLayer to tally votes in TEE**

### View Results

```bash
near view my-dao.YOURACCOUNT.testnet get_proposal \
  '{"proposal_id": 1}'
```

**Expected**: Proposal with `tally_result` containing yes/no counts

---

## Step 6: Setup Frontend

```bash
cd dao-frontend

# Copy environment config
cp .env.example .env

# Edit .env - set your contract ID
# REACT_APP_CONTRACT_ID=my-dao.YOURACCOUNT.testnet

# Install dependencies
npm install

# Start development server
npm start
```

**Open**: http://localhost:3000

---

## Step 7: Test Frontend Flow

1. **Connect Wallet**
   - Click "Connect Wallet"
   - Sign in with NEAR wallet

2. **Join DAO**
   - Go to "Join DAO" tab
   - Click "Join DAO (0.012 NEAR)"
   - Approve transaction
   - Wait for key generation

3. **Create Proposal**
   - Go to "Create Proposal" tab
   - Fill in title, description, quorum, deadline
   - Submit

4. **Vote**
   - Go to "Vote" tab
   - Select proposal
   - Click YES or NO
   - Submit vote

5. **View Results**
   - Go to "Proposals" tab
   - Wait for deadline
   - Finalize proposal
   - View tally results with TEE attestation

---

## 🐛 Troubleshooting

### Contract deployment fails
```
Error: Account my-dao.YOURACCOUNT.testnet already exists
```
**Solution**: Use a different account name or delete existing account

### OutLayer callback never arrives
```
User has membership but no public key after 2+ minutes
```
**Possible causes**:
1. OutLayer worker not running
2. Master secret not configured
3. WASI module not uploaded to GitHub

**Solution**: Check OutLayer worker logs, verify GitHub repo URL

### Frontend won't connect
```
Error: Failed to fetch DAO info
```
**Solution**: Verify `.env` has correct contract ID and network

### Encryption not working
```
Warning: Using placeholder encryption!
```
**Status**: EXPECTED - MVP uses simplified encryption
**Fix**: See ISSUES_AND_TODOS.md #1 - Full ECIES needed

---

## 📝 Next Steps After MVP

### Before Testnet Production:
- [ ] Implement full ECIES encryption (issue #1)
- [ ] Setup proper keymaster secrets
- [ ] Add NEP-297 events
- [ ] Add pagination for proposals/members
- [ ] Test with 10+ users and 50+ proposals

### Before Mainnet:
- [ ] Security audit (contract + WASI + frontend)
- [ ] Real TEE attestation (SGX/SEV)
- [ ] Zero-knowledge proofs (optional but recommended)
- [ ] Gas optimization
- [ ] Load testing
- [ ] Documentation and tutorials

---

## 🔐 Security Considerations

### MVP Security Model:
- ✅ Votes encrypted on-chain (even if simplified encryption)
- ✅ Tallying happens in TEE (worker isolation)
- ✅ Individual votes never revealed
- ⚠️ Encryption is placeholder (not production-ready)
- ⚠️ TEE attestation is hash-based (not hardware-verified)

### Production Security Model:
- Full ECIES with secp256k1
- Real SGX/SEV attestation
- Optional ZK proofs for mathematical verification
- Rate limiting and DoS protection
- Membership revocation
- Master secret rotation support

---

## 💰 Cost Estimates (Testnet)

| Action | Cost | Notes |
|--------|------|-------|
| Join DAO (Public) | ~0.012 NEAR | Storage + OutLayer key derivation |
| Complete Join (Private) | ~0.01 NEAR | OutLayer key derivation only |
| Create Proposal | ~0.001 NEAR | Storage only |
| Cast Vote | ~0.002 NEAR | Storage (non-refundable) |
| Finalize Proposal | ~0.01 NEAR | OutLayer tallying in TEE |

**Total for one voting cycle**: ~0.025 NEAR per user

---

## 📚 Additional Resources

- [ARCHITECTURE.md](ARCHITECTURE.md) - Technical specification
- [TEST_RESULTS.md](TEST_RESULTS.md) - WASI module tests
- [ISSUES_AND_TODOS.md](ISSUES_AND_TODOS.md) - Known issues and roadmap
- [dao-contract/src/lib.rs](dao-contract/src/lib.rs) - Contract source code
- [dao-frontend/src/App.tsx](dao-frontend/src/App.tsx) - Frontend source

---

**Last Updated**: 2025-11-07
**Version**: MVP v0.1.0
**Status**: ✅ Ready for testnet testing (with known limitations)
