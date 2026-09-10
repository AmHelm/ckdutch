import assert from 'node:assert/strict'
import { readFile } from 'node:fs/promises'
import test from 'node:test'
import init, { CkdRequest, validateCapsule } from './pkg/ckdutch_crypto.js'

await init({ module_or_path: await readFile(new URL('./pkg/ckdutch_crypto_bg.wasm', import.meta.url)) })

const capsule = {
  version: 1,
  network: 'testnet',
  contract: 'owner.testnet',
  mpc_contract: 'v1.signer-prod.testnet',
  domain_id: 2,
  derivation_path: 'd',
  mpc_public_key: 'test-key',
  algorithm: 'AES-256-GCM/HKDF-SHA256',
  filename: 'pro tebe 🗝️.txt',
  nonce: 'AAAAAAAAAAAAAAAA',
  ciphertext: '',
}

test('WASM validates capsule metadata and rejects unsupported fields and parameters', () => {
  assert.deepEqual(JSON.parse(validateCapsule(JSON.stringify(capsule))), capsule)
  for (const changed of [
    { ...capsule, extra: true },
    { ...capsule, network: 'mainnet' },
    { ...capsule, domain_id: 3 },
    { ...capsule, filename: '🗝'.repeat(86) },
  ]) {
    assert.throws(() => validateCapsule(JSON.stringify(changed)))
  }
})

test('WASM creates fresh request keys and rejects invalid MPC responses', () => {
  const first = new CkdRequest()
  const second = new CkdRequest()
  try {
    assert.match(first.publicKey(), /^bls12381g1:/)
    assert.notEqual(first.publicKey(), second.publicKey())
    assert.throws(() => first.derive('{}', 'owner.testnet', 'invalid'))
    assert.throws(() => first.derive('{"big_c":"invalid","big_y":"invalid"}', 'owner.testnet', 'invalid'))
  } finally {
    first.free()
    second.free()
  }
})
