#!/bin/bash
# Full end-to-end test: key derivation, encryption, and vote tallying
#
# This demonstrates the complete private voting flow:
# 1. Derive public keys for users
# 2. Encrypt votes with those keys (simulated client-side)
# 3. Tally votes in TEE (worker-side)

set -e

WASM_FILE="target/wasm32-wasip1/release/private-dao-example.wasm"
WASI_TEST="../wasi-test-runner/target/release/wasi-test"
MASTER="0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
DAO="dao.testnet"

# Check if WASM file exists
if [ ! -f "$WASM_FILE" ]; then
    echo "❌ WASM file not found. Run ./build.sh first."
    exit 1
fi

# Check if wasi-test runner exists
if [ ! -f "$WASI_TEST" ]; then
    echo "❌ WASI test runner not found at $WASI_TEST"
    exit 1
fi

echo "🗳️  Private DAO Voting - Full Cycle Test"
echo "========================================"
echo ""

# Ask the module for a voter's public key, exactly as a client would.
derive_pubkey() {
    "$WASI_TEST" --wasm "$WASM_FILE" \
        --input "{\"action\":\"derive_pubkey\",\"dao_account\":\"$DAO\",\"user_account\":\"$1\"}" \
        --env PROTECTED_DAO_MASTER_SECRET=$MASTER 2>&1 \
      | grep -A 1000 "^Output:" | tail -n +2 | head -n 1 | jq -r '.result.pubkey'
}

# Step 1: Generate encrypted votes
echo "📝 Step 1: Generating encrypted votes..."
echo ""

# The voter only ever sees a public key; the private half exists solely inside the TEE.
python3 encrypt_test_vote.py "$(derive_pubkey alice.testnet)" alice.testnet yes > /tmp/alice_vote.json
python3 encrypt_test_vote.py "$(derive_pubkey bob.testnet)" bob.testnet no > /tmp/bob_vote.json
python3 encrypt_test_vote.py "$(derive_pubkey carol.testnet)" carol.testnet yes > /tmp/carol_vote.json
python3 encrypt_test_vote.py "$(derive_pubkey dave.testnet)" dave.testnet "noise_123" 2>/dev/null > /tmp/dave_vote.json

echo "✅ Votes encrypted (client-side simulation)"
echo "   - alice.testnet: YES (encrypted)"
echo "   - bob.testnet: NO (encrypted)"
echo "   - carol.testnet: YES (encrypted)"
echo "   - dave.testnet: DUMMY/NOISE (encrypted)"
echo ""

# Step 2: Store votes "on-chain" (simulated)
echo "📦 Step 2: Votes stored on DAO contract (simulated)"
echo "   In production: Contract stores encrypted votes with timestamps"
echo ""

# Step 3: Tally votes in TEE
echo "🔒 Step 3: Tallying votes in TEE (OutLayer Worker)..."
echo ""

# Extract vote data for JSON. No separate nonce: ECIES carries it inside the ciphertext.
ALICE_ENC=$(jq -r .encrypted_vote /tmp/alice_vote.json)
BOB_ENC=$(jq -r .encrypted_vote /tmp/bob_vote.json)
CAROL_ENC=$(jq -r .encrypted_vote /tmp/carol_vote.json)
DAVE_ENC=$(jq -r .encrypted_vote /tmp/dave_vote.json)

# Create tally input
TALLY_INPUT=$(cat <<EOF
{
  "action": "tally_votes",
  "dao_account": "$DAO",
  "proposal_id": 1,
  "votes": [
    {
      "user": "alice.testnet",
      "encrypted_vote": "$ALICE_ENC",
      "timestamp": 1700000000
    },
    {
      "user": "bob.testnet",
      "encrypted_vote": "$BOB_ENC",
      "timestamp": 1700000001
    },
    {
      "user": "carol.testnet",
      "encrypted_vote": "$CAROL_ENC",
      "timestamp": 1700000002
    },
    {
      "user": "dave.testnet",
      "encrypted_vote": "$DAVE_ENC",
      "timestamp": 1700000003
    }
  ],
  "quorum": { "Absolute": { "min_votes": 3 } }
}
EOF
)

# Run tallying
RESULT=$("$WASI_TEST" --wasm "$WASM_FILE" \
  --input "$TALLY_INPUT" \
  --env PROTECTED_DAO_MASTER_SECRET=$MASTER 2>&1)

# Extract just the output JSON. The runner prints a bare "Output:" header and the JSON
# on the following line; "Output size:" above it does not match this pattern.
OUTPUT=$(echo "$RESULT" | grep -A 1000 "^Output:" | tail -n +2 | head -n 1)

echo "✅ Tallying complete!"
echo ""

# Parse results
YES_COUNT=$(echo "$OUTPUT" | jq -r '.result.yes_count')
NO_COUNT=$(echo "$OUTPUT" | jq -r '.result.no_count')
TOTAL=$(echo "$OUTPUT" | jq -r '.result.total_votes')
TEE_ATT=$(echo "$OUTPUT" | jq -r '.result.tee_attestation')

echo "📊 Results:"
echo "   YES votes: $YES_COUNT"
echo "   NO votes:  $NO_COUNT"
echo "   Total:     $TOTAL"
echo ""
echo "🔐 TEE Attestation:"
echo "   $TEE_ATT"
echo ""

# Verify expected results
if [ "$YES_COUNT" -eq 2 ] && [ "$NO_COUNT" -eq 1 ] && [ "$TOTAL" -eq 3 ]; then
    echo "✅ All tests passed!"
    echo ""
    echo "🎉 Summary:"
    echo "   - 3 real votes counted (alice, bob, carol)"
    echo "   - 1 dummy message ignored (dave)"
    echo "   - Individual votes remain private (only tallies revealed)"
    echo "   - TEE attestation proves correct execution"
else
    echo "❌ Test failed! Unexpected vote counts."
    echo "   Expected: YES=2, NO=1, TOTAL=3"
    echo "   Got:      YES=$YES_COUNT, NO=$NO_COUNT, TOTAL=$TOTAL"
    exit 1
fi
