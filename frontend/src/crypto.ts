export type { CapsuleKey } from "../crypto/pkg/ckdutch_crypto.js"

type CryptoModule = typeof import("../crypto/pkg/ckdutch_crypto.js")
let pending: Promise<CryptoModule> | undefined

/** Load the existing Rust cryptography only when a capsule operation needs it. */
export function loadCrypto(): Promise<CryptoModule> {
  pending ??= import("../crypto/pkg/ckdutch_crypto.js")
    .then(async (module) => {
      await module.default()
      return module
    })
    .catch((error: unknown) => {
      pending = undefined
      throw error
    })
  return pending
}
