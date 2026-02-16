import { Buffer } from "buffer";
import { Address } from "@stellar/stellar-sdk";
import {
  AssembledTransaction,
  Client as ContractClient,
  ClientOptions as ContractClientOptions,
  MethodOptions,
  Result,
  Spec as ContractSpec,
} from "@stellar/stellar-sdk/contract";
import type {
  u32,
  i32,
  u64,
  i64,
  u128,
  i128,
  u256,
  i256,
  Option,
  Timepoint,
  Duration,
} from "@stellar/stellar-sdk/contract";
export * from "@stellar/stellar-sdk";
export * as contract from "@stellar/stellar-sdk/contract";
export * as rpc from "@stellar/stellar-sdk/rpc";

if (typeof window !== "undefined") {
  //@ts-ignore Buffer exists
  window.Buffer = window.Buffer || Buffer;
}


export const networks = {
  testnet: {
    networkPassphrase: "Test SDF Network ; September 2015",
    contractId: "CAKVUHDKKEG6SYUAVMQMDRMUGCNQJS74BP45NNYS7Y2TTYUMYFSLA7EU",
  }
} as const

export const ScoreError = {
  1: {message:"InvalidJournalLength"},
  2: {message:"InvalidRulesDigest"},
  3: {message:"JournalAlreadyClaimed"},
  4: {message:"ZeroScoreNotAllowed"},
  5: {message:"ScoreNotImproved"}
}


export interface Client {
  /**
   * Construct and simulate a image_id transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Read the current image ID.
   */
  image_id: (options?: MethodOptions) => Promise<AssembledTransaction<Buffer>>

  /**
   * Construct and simulate a token_id transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Read the token address.
   */
  token_id: (options?: MethodOptions) => Promise<AssembledTransaction<string>>

  /**
   * Construct and simulate a router_id transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Read the router address.
   */
  router_id: (options?: MethodOptions) => Promise<AssembledTransaction<string>>

  /**
   * Construct and simulate a set_admin transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Admin: transfer admin role.
   */
  set_admin: ({new_admin}: {new_admin: string}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a best_score transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Read a claimant's best score for a seed.
   */
  best_score: ({claimant, seed}: {claimant: string, seed: u32}, options?: MethodOptions) => Promise<AssembledTransaction<u32>>

  /**
   * Construct and simulate a is_claimed transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Check whether a journal digest has already been claimed.
   */
  is_claimed: ({journal_digest}: {journal_digest: Buffer}, options?: MethodOptions) => Promise<AssembledTransaction<boolean>>

  /**
   * Construct and simulate a rules_digest transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Read the expected rules digest.
   */
  rules_digest: (options?: MethodOptions) => Promise<AssembledTransaction<u32>>

  /**
   * Construct and simulate a set_image_id transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Admin: update the image ID (for program upgrades).
   */
  set_image_id: ({new_image_id}: {new_image_id: Buffer}, options?: MethodOptions) => Promise<AssembledTransaction<null>>

  /**
   * Construct and simulate a submit_score transaction. Returns an `AssembledTransaction` object which will have a `result` field containing the result of the simulation. If this transaction changes contract state, you will need to call `signAndSend()` on the returned object.
   * Verify a RISC Zero proof and mint score tokens to the claimant address.
   * 
   * - `seal`: variable-length proof seal bytes
   * - `journal_raw`: raw 24-byte journal bytes (6 × u32 LE)
   * - `claimant`: recipient address for token minting and best-score tracking
   * 
   * Returns the claimant's new best score for this seed.
   */
  submit_score: ({seal, journal_raw, claimant}: {seal: Buffer, journal_raw: Buffer, claimant: string}, options?: MethodOptions) => Promise<AssembledTransaction<Result<u32>>>

}
export class Client extends ContractClient {
  static async deploy<T = Client>(
        /** Constructor/Initialization Args for the contract's `__constructor` method */
        {admin, router_id, image_id, token_id}: {admin: string, router_id: string, image_id: Buffer, token_id: string},
    /** Options for initializing a Client as well as for calling a method, with extras specific to deploying. */
    options: MethodOptions &
      Omit<ContractClientOptions, "contractId"> & {
        /** The hash of the Wasm blob, which must already be installed on-chain. */
        wasmHash: Buffer | string;
        /** Salt used to generate the contract's ID. Passed through to {@link Operation.createCustomContract}. Default: random. */
        salt?: Buffer | Uint8Array;
        /** The format used to decode `wasmHash`, if it's provided as a string. */
        format?: "hex" | "base64";
      }
  ): Promise<AssembledTransaction<T>> {
    return ContractClient.deploy({admin, router_id, image_id, token_id}, options)
  }
  constructor(public readonly options: ContractClientOptions) {
    super(
      new ContractSpec([ "AAAABAAAAAAAAAAAAAAAClNjb3JlRXJyb3IAAAAAAAUAAAAAAAAAFEludmFsaWRKb3VybmFsTGVuZ3RoAAAAAQAAAAAAAAASSW52YWxpZFJ1bGVzRGlnZXN0AAAAAAACAAAAAAAAABVKb3VybmFsQWxyZWFkeUNsYWltZWQAAAAAAAADAAAAAAAAABNaZXJvU2NvcmVOb3RBbGxvd2VkAAAAAAQAAAAAAAAAEFNjb3JlTm90SW1wcm92ZWQAAAAF",
        "AAAABQAAAAAAAAAAAAAADlNjb3JlU3VibWl0dGVkAAAAAAABAAAAD3Njb3JlX3N1Ym1pdHRlZAAAAAAGAAAAAAAAAAhjbGFpbWFudAAAABMAAAAAAAAAAAAAAARzZWVkAAAABAAAAAAAAAAAAAAADXByZXZpb3VzX2Jlc3QAAAAAAAAEAAAAAAAAAAAAAAAIbmV3X2Jlc3QAAAAEAAAAAAAAAAAAAAAMbWludGVkX2RlbHRhAAAABAAAAAAAAAAAAAAADmpvdXJuYWxfZGlnZXN0AAAAAAPuAAAAIAAAAAAAAAAC",
        "AAAAAAAAABpSZWFkIHRoZSBjdXJyZW50IGltYWdlIElELgAAAAAACGltYWdlX2lkAAAAAAAAAAEAAAPuAAAAIA==",
        "AAAAAAAAABdSZWFkIHRoZSB0b2tlbiBhZGRyZXNzLgAAAAAIdG9rZW5faWQAAAAAAAAAAQAAABM=",
        "AAAAAAAAABhSZWFkIHRoZSByb3V0ZXIgYWRkcmVzcy4AAAAJcm91dGVyX2lkAAAAAAAAAAAAAAEAAAAT",
        "AAAAAAAAABtBZG1pbjogdHJhbnNmZXIgYWRtaW4gcm9sZS4AAAAACXNldF9hZG1pbgAAAAAAAAEAAAAAAAAACW5ld19hZG1pbgAAAAAAABMAAAAA",
        "AAAAAAAAAChSZWFkIGEgY2xhaW1hbnQncyBiZXN0IHNjb3JlIGZvciBhIHNlZWQuAAAACmJlc3Rfc2NvcmUAAAAAAAIAAAAAAAAACGNsYWltYW50AAAAEwAAAAAAAAAEc2VlZAAAAAQAAAABAAAABA==",
        "AAAAAAAAADhDaGVjayB3aGV0aGVyIGEgam91cm5hbCBkaWdlc3QgaGFzIGFscmVhZHkgYmVlbiBjbGFpbWVkLgAAAAppc19jbGFpbWVkAAAAAAABAAAAAAAAAA5qb3VybmFsX2RpZ2VzdAAAAAAD7gAAACAAAAABAAAAAQ==",
        "AAAAAAAAAB9SZWFkIHRoZSBleHBlY3RlZCBydWxlcyBkaWdlc3QuAAAAAAxydWxlc19kaWdlc3QAAAAAAAAAAQAAAAQ=",
        "AAAAAAAAADJBZG1pbjogdXBkYXRlIHRoZSBpbWFnZSBJRCAoZm9yIHByb2dyYW0gdXBncmFkZXMpLgAAAAAADHNldF9pbWFnZV9pZAAAAAEAAAAAAAAADG5ld19pbWFnZV9pZAAAA+4AAAAgAAAAAA==",
        "AAAAAAAAASxWZXJpZnkgYSBSSVNDIFplcm8gcHJvb2YgYW5kIG1pbnQgc2NvcmUgdG9rZW5zIHRvIHRoZSBjbGFpbWFudCBhZGRyZXNzLgoKLSBgc2VhbGA6IHZhcmlhYmxlLWxlbmd0aCBwcm9vZiBzZWFsIGJ5dGVzCi0gYGpvdXJuYWxfcmF3YDogcmF3IDI0LWJ5dGUgam91cm5hbCBieXRlcyAoNiDDlyB1MzIgTEUpCi0gYGNsYWltYW50YDogcmVjaXBpZW50IGFkZHJlc3MgZm9yIHRva2VuIG1pbnRpbmcgYW5kIGJlc3Qtc2NvcmUgdHJhY2tpbmcKClJldHVybnMgdGhlIGNsYWltYW50J3MgbmV3IGJlc3Qgc2NvcmUgZm9yIHRoaXMgc2VlZC4AAAAMc3VibWl0X3Njb3JlAAAAAwAAAAAAAAAEc2VhbAAAAA4AAAAAAAAAC2pvdXJuYWxfcmF3AAAAAA4AAAAAAAAACGNsYWltYW50AAAAEwAAAAEAAAPpAAAABAAAB9AAAAAKU2NvcmVFcnJvcgAA",
        "AAAAAAAAAAAAAAANX19jb25zdHJ1Y3RvcgAAAAAAAAQAAAAAAAAABWFkbWluAAAAAAAAEwAAAAAAAAAJcm91dGVyX2lkAAAAAAAAEwAAAAAAAAAIaW1hZ2VfaWQAAAPuAAAAIAAAAAAAAAAIdG9rZW5faWQAAAATAAAAAA==" ]),
      options
    )
  }
  public readonly fromJSON = {
    image_id: this.txFromJSON<Buffer>,
        token_id: this.txFromJSON<string>,
        router_id: this.txFromJSON<string>,
        set_admin: this.txFromJSON<null>,
        best_score: this.txFromJSON<u32>,
        is_claimed: this.txFromJSON<boolean>,
        rules_digest: this.txFromJSON<u32>,
        set_image_id: this.txFromJSON<null>,
        submit_score: this.txFromJSON<Result<u32>>
  }
}
