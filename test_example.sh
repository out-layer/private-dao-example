#!/bin/bash
# Example: Test private-dao-example WASI module locally with wasi-test runner
#
# This covers key derivation and the module's behaviour on undecryptable input.
# For the real end-to-end path (derive key -> encrypt vote -> tally), run
# ./test_full_cycle.sh, which encrypts with the same ECIES scheme the TEE decrypts.

set -e

WASM_FILE="target/wasm32-wasip1/release/private-dao-example.wasm"
WASI_TEST="../wasi-test-runner/target/release/wasi-test"
MASTER="0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"

# Check if WASM file exists
if [ ! -f "$WASM_FILE" ]; then
    echo "❌ WASM file not found. Run ./build.sh first."
    exit 1
fi

# Check if wasi-test runner exists
if [ ! -f "$WASI_TEST" ]; then
    echo "❌ WASI test runner not found at $WASI_TEST"
    echo "Build it with: cd ../wasi-test-runner && cargo build --release"
    exit 1
fi

# Run the module and return only the output JSON. The runner prints a bare "Output:"
# header with the JSON on the next line.
run_module() {
    "$WASI_TEST" --wasm "$WASM_FILE" --input "$1" --env PROTECTED_DAO_MASTER_SECRET=$MASTER 2>&1 \
      | grep -A 1000 "^Output:" | tail -n +2 | head -n 1
}

# Test 1 & 2: Derive public keys for two users
echo "Test 1: Derive public key for alice.testnet"
ALICE_OUT=$(run_module '{"action":"derive_pubkey","dao_account":"dao.testnet","user_account":"alice.testnet"}')
ALICE_PUB=$(echo "$ALICE_OUT" | jq -r '.result.pubkey')
echo "  $ALICE_PUB"
echo ""

echo "Test 2: Derive public key for bob.testnet (must differ from alice)"
BOB_OUT=$(run_module '{"action":"derive_pubkey","dao_account":"dao.testnet","user_account":"bob.testnet"}')
BOB_PUB=$(echo "$BOB_OUT" | jq -r '.result.pubkey')
echo "  $BOB_PUB"
echo ""

if [ "$ALICE_PUB" = "null" ] || [ "$BOB_PUB" = "null" ]; then
    echo "❌ Key derivation failed"
    echo "   alice: $ALICE_OUT"
    echo "   bob:   $BOB_OUT"
    exit 1
fi

if [ "$ALICE_PUB" = "$BOB_PUB" ]; then
    echo "❌ Both users derived the same key — per-voter privacy is broken"
    exit 1
fi

# 33-byte compressed secp256k1 key = 66 hex characters
if [ ${#ALICE_PUB} -ne 66 ]; then
    echo "❌ Expected a 33-byte compressed public key, got ${#ALICE_PUB} hex chars"
    exit 1
fi

echo "---"
echo ""

# Test 3: Undecryptable votes must be skipped, not fatal
echo "Test 3: Votes that fail to decrypt are ignored"
echo ""
echo "These ciphertexts are deliberate garbage. A vote the TEE cannot decrypt must not"
echo "abort the tally — otherwise one malformed submission censors the whole proposal."
echo ""

INPUT3='{
  "action": "tally_votes",
  "dao_account": "dao.testnet",
  "proposal_id": 1,
  "votes": [
    {
      "user": "alice.testnet",
      "encrypted_vote": "739a7a3aeb7d39d3b84e3eaf8b",
      "timestamp": 1700000000
    },
    {
      "user": "bob.testnet",
      "encrypted_vote": "739a7a3aeb7d39d3b84e3eaf8b",
      "timestamp": 1700000001
    }
  ],
  "quorum": { "Absolute": { "min_votes": 1 } }
}'

OUT3=$(run_module "$INPUT3")
SUCCESS=$(echo "$OUT3" | jq -r '.success')
TOTAL=$(echo "$OUT3" | jq -r '.result.total_votes')

echo "  success: $SUCCESS, total_votes: $TOTAL"
echo ""

if [ "$SUCCESS" != "true" ] || [ "$TOTAL" != "0" ]; then
    echo "❌ Expected a successful tally with 0 counted votes, got: $OUT3"
    exit 1
fi

echo "✅ All tests passed!"
echo ""
echo "Summary:"
echo "  - alice.testnet pubkey: $ALICE_PUB"
echo "  - bob.testnet pubkey:   $BOB_PUB"
echo "  - Different users get different keys (privacy ✓)"
echo "  - Undecryptable votes are skipped, not fatal (availability ✓)"
echo ""
echo "Run ./test_full_cycle.sh for the full encrypt-and-tally path."
