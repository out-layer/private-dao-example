#!/usr/bin/env python3
"""
Helper script to encrypt votes for testing private-dao-example.

This simulates the client-side encryption that happens in the browser: a voter
encrypts to the public key the TEE derived for them, and only the TEE — which alone
holds PROTECTED_DAO_MASTER_SECRET and can therefore re-derive the private key — can
read the vote back.

The wire format is whatever the `ecies` crate (v0.2, default config, `pure` feature)
produces, because that is what src/crypto.rs calls to decrypt:

    ephemeral_pubkey (65, uncompressed SEC1) || nonce (16) || tag (16) || ciphertext

with the symmetric key derived as

    HKDF-SHA256(salt = zeros, ikm = ephemeral_pubkey || shared_point, info = "")

where both points are uncompressed SEC1 and shared_point = receiver_pubkey *
ephemeral_secret. The AEAD is AES-256-GCM with an empty AAD and a 16-byte nonce (the
crate uses 16, not the more common 12, unless built with `aes-short-nonce`).

Reimplementing the curve arithmetic here rather than pulling in a secp256k1 binding
keeps the example runnable with nothing but `cryptography` installed.
"""

import json
import os
import secrets
import sys

from cryptography.hazmat.primitives.ciphers.aead import AESGCM
from cryptography.hazmat.primitives.hashes import SHA256
from cryptography.hazmat.primitives.kdf.hkdf import HKDF

# secp256k1 domain parameters
P = 2**256 - 2**32 - 977
N = 0xFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFEBAAEDCE6AF48A03BBFD25E8CD0364141
GX = 0x79BE667EF9DCBBAC55A06295CE870B07029BFCDB2DCE28D959F2815B16F81798
GY = 0x483ADA7726A3C4655DA4FBFC0E1108A8FD17B448A68554199C47D08FFB10D4B8


def point_add(p1, p2):
    """Add two points on secp256k1. None is the point at infinity."""
    if p1 is None:
        return p2
    if p2 is None:
        return p1
    x1, y1 = p1
    x2, y2 = p2
    if x1 == x2 and (y1 + y2) % P == 0:
        return None
    if p1 == p2:
        lam = (3 * x1 * x1) * pow(2 * y1, P - 2, P) % P
    else:
        lam = (y2 - y1) * pow(x2 - x1, P - 2, P) % P
    x3 = (lam * lam - x1 - x2) % P
    return (x3, (lam * (x1 - x3) - y1) % P)


def point_mul(point, scalar):
    """Multiply a point by a scalar (double-and-add)."""
    result = None
    addend = point
    while scalar:
        if scalar & 1:
            result = point_add(result, addend)
        addend = point_add(addend, addend)
        scalar >>= 1
    return result


def decompress(pubkey: bytes):
    """Parse a 33-byte compressed SEC1 public key into an (x, y) point."""
    if len(pubkey) != 33 or pubkey[0] not in (2, 3):
        raise ValueError(f"expected a 33-byte compressed public key, got {pubkey.hex()}")
    x = int.from_bytes(pubkey[1:], "big")
    y = pow((x * x * x + 7) % P, (P + 1) // 4, P)
    if y % 2 != pubkey[0] % 2:
        y = P - y
    return (x, y)


def serialize_uncompressed(point) -> bytes:
    x, y = point
    return b"\x04" + x.to_bytes(32, "big") + y.to_bytes(32, "big")


def ecies_encrypt(receiver_pubkey: bytes, plaintext: bytes) -> bytes:
    """Encrypt to a compressed secp256k1 public key, matching the `ecies` crate."""
    receiver_point = decompress(receiver_pubkey)

    ephemeral_sk = secrets.randbelow(N - 1) + 1
    ephemeral_pk = serialize_uncompressed(point_mul((GX, GY), ephemeral_sk))
    shared_point = serialize_uncompressed(point_mul(receiver_point, ephemeral_sk))

    sym_key = HKDF(algorithm=SHA256(), length=32, salt=None, info=b"").derive(
        ephemeral_pk + shared_point
    )

    nonce = os.urandom(16)
    # `cryptography` returns ciphertext||tag; the crate stores the tag before the body.
    sealed = AESGCM(sym_key).encrypt(nonce, plaintext, None)
    ciphertext, tag = sealed[:-16], sealed[-16:]

    return ephemeral_pk + nonce + tag + ciphertext


def main():
    if len(sys.argv) != 4:
        print("Usage: encrypt_test_vote.py <user_pubkey_hex> <user_account> <vote>")
        print()
        print("The public key is the one the module returns for this user from its")
        print("derive_pubkey action — it is not a secret, which is the whole point.")
        print()
        print("Example:")
        print("  python3 encrypt_test_vote.py \\")
        print("    02a1b2c3... \\")
        print("    alice.testnet \\")
        print("    yes")
        sys.exit(1)

    pubkey_hex, user_account, vote = sys.argv[1], sys.argv[2], sys.argv[3]

    if vote not in ["yes", "no"]:
        print(
            f"Warning: vote '{vote}' is not 'yes' or 'no' - it will be treated as dummy/noise",
            file=sys.stderr,
        )

    encrypted = ecies_encrypt(bytes.fromhex(pubkey_hex), vote.encode("utf-8"))

    result = {
        "user": user_account,
        "encrypted_vote": encrypted.hex(),
        "timestamp": 1700000000,  # Placeholder timestamp
    }

    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
