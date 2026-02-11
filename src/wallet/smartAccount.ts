import {
  IndexedDBStorage,
  SmartAccountKit,
  validateAddress,
  type ConnectWalletResult,
} from "smart-account-kit";
import {
  ChannelsClient,
  PluginExecutionError,
  PluginTransportError,
} from "@openzeppelin/relayer-plugin-channels/dist/client";

const TESTNET_NETWORK_PASSPHRASE = "Test SDF Network ; September 2015";
const PUBLIC_NETWORK_PASSPHRASE = "Public Global Stellar Network ; September 2015";
const TESTNET_RPC_URL = "https://soroban-testnet.stellar.org";
const TESTNET_ACCOUNT_WASM_HASH =
  "a12e8fa9621efd20315753bd4007d974390e31fbcb4a7ddc4dd0a0dec728bf2e";
const TESTNET_WEBAUTHN_VERIFIER = "CBSHV66WG7UV6FQVUTB67P3DZUEJ2KJ5X6JKQH5MFRAAFNFJUAJVXJYV";
const OPENZEPPELIN_CHANNELS_MAINNET_URL = "https://channels.openzeppelin.com";
const OPENZEPPELIN_CHANNELS_TESTNET_URL = "https://channels.openzeppelin.com/testnet";
const DEFAULT_RP_NAME = "Stellar ZK";
const DEFAULT_USER_NAME = "Player";
const APP_NAME = "Stellar ZK Asteroids";

export interface SmartWalletSession {
  contractId: string;
  credentialId: string;
}

export interface SmartAccountConfig {
  rpcUrl: string;
  networkPassphrase: string;
  accountWasmHash: string;
  webauthnVerifierAddress: string;
  relayerUrl: string | null;
  relayerApiKey: string | null;
  relayerPluginId: string | null;
  rpName: string;
}

export type SmartAccountRelayerMode =
  | "channels-api-key"
  | "channels-missing-key"
  | "proxy"
  | "disabled";

function getEnvValue(key: keyof ImportMetaEnv): string | undefined {
  const value = import.meta.env[key];
  if (typeof value === "string" && value.trim().length > 0) {
    return value.trim();
  }

  return undefined;
}

function defaultChannelsUrlForNetwork(networkPassphrase: string): string {
  if (networkPassphrase === TESTNET_NETWORK_PASSPHRASE) {
    return OPENZEPPELIN_CHANNELS_TESTNET_URL;
  }

  if (networkPassphrase === PUBLIC_NETWORK_PASSPHRASE) {
    return OPENZEPPELIN_CHANNELS_MAINNET_URL;
  }

  return OPENZEPPELIN_CHANNELS_TESTNET_URL;
}

const configuredNetworkPassphrase =
  getEnvValue("VITE_SMART_ACCOUNT_NETWORK_PASSPHRASE") ?? TESTNET_NETWORK_PASSPHRASE;

const config: SmartAccountConfig = {
  rpcUrl: getEnvValue("VITE_SMART_ACCOUNT_RPC_URL") ?? TESTNET_RPC_URL,
  networkPassphrase: configuredNetworkPassphrase,
  accountWasmHash: getEnvValue("VITE_SMART_ACCOUNT_WASM_HASH") ?? TESTNET_ACCOUNT_WASM_HASH,
  webauthnVerifierAddress:
    getEnvValue("VITE_SMART_ACCOUNT_WEBAUTHN_VERIFIER_ADDRESS") ?? TESTNET_WEBAUTHN_VERIFIER,
  relayerUrl:
    getEnvValue("VITE_SMART_ACCOUNT_RELAYER_URL") ??
    defaultChannelsUrlForNetwork(configuredNetworkPassphrase),
  relayerApiKey: getEnvValue("VITE_SMART_ACCOUNT_RELAYER_API_KEY") ?? null,
  relayerPluginId: getEnvValue("VITE_SMART_ACCOUNT_RELAYER_PLUGIN_ID") ?? null,
  rpName: getEnvValue("VITE_SMART_ACCOUNT_RP_NAME") ?? DEFAULT_RP_NAME,
};

let kitInstance: SmartAccountKit | null = null;

function ensureClaimantContractAddress(contractId: string): string {
  const normalized = contractId.trim();
  validateAddress(normalized, "claimant contract address");

  if (!normalized.startsWith("C")) {
    throw new Error(`claimant must be a smart-account contract address (got "${normalized}")`);
  }

  return normalized;
}

function toWalletSession(result: ConnectWalletResult): SmartWalletSession {
  return {
    contractId: ensureClaimantContractAddress(result.contractId),
    credentialId: result.credentialId,
  };
}

function formatRelayerError(error: unknown): string {
  if (error instanceof PluginExecutionError) {
    const errorCode =
      typeof error.errorDetails?.code === "string" ? ` (${error.errorDetails.code})` : "";
    return `${error.message}${errorCode}`;
  }

  if (error instanceof PluginTransportError) {
    if (typeof error.statusCode === "number") {
      return `${error.message} (status ${error.statusCode})`;
    }
    return error.message;
  }

  if (error instanceof Error) {
    return error.message;
  }

  return String(error);
}

export function getSmartAccountConfig(): SmartAccountConfig {
  return { ...config };
}

export function getSmartAccountRelayerMode(): SmartAccountRelayerMode {
  if (!config.relayerUrl) {
    return "disabled";
  }

  if (config.relayerApiKey) {
    return "channels-api-key";
  }

  if (config.relayerUrl.includes("channels.openzeppelin.com")) {
    return "channels-missing-key";
  }

  if (config.relayerUrl) {
    return "proxy";
  }

  return "disabled";
}

export function getSmartAccountKit(): SmartAccountKit {
  if (!kitInstance) {
    kitInstance = new SmartAccountKit({
      rpcUrl: config.rpcUrl,
      networkPassphrase: config.networkPassphrase,
      accountWasmHash: config.accountWasmHash,
      webauthnVerifierAddress: config.webauthnVerifierAddress,
      storage: new IndexedDBStorage(),
      rpName: config.rpName,
      relayerUrl: config.relayerUrl ?? undefined,
    });
  }

  return kitInstance;
}

async function submitDeploymentXdr(signedTransaction: string): Promise<void> {
  const relayerUrl = config.relayerUrl?.trim() ?? "";
  const relayerApiKey = config.relayerApiKey?.trim() ?? "";

  if (relayerUrl.length === 0) {
    throw new Error(
      "missing relayer URL; set VITE_SMART_ACCOUNT_RELAYER_URL for smart wallet deployment",
    );
  }

  if (relayerApiKey.length > 0) {
    const client = new ChannelsClient({
      baseUrl: relayerUrl,
      apiKey: relayerApiKey,
      pluginId: config.relayerPluginId ?? undefined,
    });
    try {
      await client.submitTransaction({ xdr: signedTransaction });
      return;
    } catch (error) {
      throw new Error(`relayer submission failed: ${formatRelayerError(error)}`, { cause: error });
    }
  }

  if (relayerUrl.includes("channels.openzeppelin.com")) {
    throw new Error(
      "VITE_SMART_ACCOUNT_RELAYER_API_KEY is required when using channels.openzeppelin.com directly",
    );
  }

  const kit = getSmartAccountKit();
  if (!kit.relayer) {
    throw new Error("relayer is not configured in smart-account-kit");
  }

  const response = await kit.relayer.sendXdr(signedTransaction);
  if (!response.success) {
    throw new Error(response.error ?? "relayer submission failed");
  }
}

export async function restoreSmartWalletSession(): Promise<SmartWalletSession | null> {
  const result = await getSmartAccountKit().connectWallet();
  return result ? toWalletSession(result) : null;
}

export async function connectSmartWallet(): Promise<SmartWalletSession> {
  const result = await getSmartAccountKit().connectWallet({ prompt: true });
  if (!result) {
    throw new Error("wallet connection was cancelled");
  }

  return toWalletSession(result);
}

export async function createSmartWallet(userName: string): Promise<SmartWalletSession> {
  const kit = getSmartAccountKit();
  const normalizedUserName = userName.trim().length > 0 ? userName.trim() : DEFAULT_USER_NAME;

  const creation = await kit.createWallet(APP_NAME, normalizedUserName, {
    autoSubmit: false,
  });

  try {
    await submitDeploymentXdr(creation.signedTransaction);
  } catch (error) {
    await kit.disconnect();
    throw error;
  }

  const connected = await kit.connectWallet({
    contractId: creation.contractId,
    credentialId: creation.credentialId,
  });
  if (!connected) {
    throw new Error("wallet deployed, but failed to restore connected session");
  }

  return toWalletSession(connected);
}

export async function disconnectSmartWallet(): Promise<void> {
  await getSmartAccountKit().disconnect();
}
