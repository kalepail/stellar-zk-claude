/// <reference types="vite/client" />

interface ImportMetaEnv {
  readonly VITE_SMART_ACCOUNT_RPC_URL?: string;
  readonly VITE_SMART_ACCOUNT_NETWORK_PASSPHRASE?: string;
  readonly VITE_SMART_ACCOUNT_WASM_HASH?: string;
  readonly VITE_SMART_ACCOUNT_WEBAUTHN_VERIFIER_ADDRESS?: string;
  readonly VITE_SMART_ACCOUNT_RELAYER_URL?: string;
  readonly VITE_SMART_ACCOUNT_RELAYER_API_KEY?: string;
  readonly VITE_SMART_ACCOUNT_RELAYER_PLUGIN_ID?: string;
  readonly VITE_SMART_ACCOUNT_RP_NAME?: string;
}

interface ImportMeta {
  readonly env: ImportMetaEnv;
}
