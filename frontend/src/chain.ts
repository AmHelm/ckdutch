import { NearConnector } from '@hot-labs/near-connect';
import { ContractNotDeployedError, FunctionCallError, fromNearConnect, Near, type FinalExecutionOutcome, type TransactionBuilder } from 'near-kit';
import { loadCrypto, type CapsuleKey } from './crypto';

export const MAX_FILE_BYTES = 10 * 1024 * 1024;
export const GLOBAL_CONTRACT = 'flying-dman.testnet';
const RPC_URL = 'https://rpc.testnet.fastnear.com';
const MPC_CONTRACT = 'v1.signer-prod.testnet';
const DOMAIN_ID = 2;
type HashCallback = (hash: string) => void;

export interface SwitchStatus {
  owner: string;
  challenge_deadline_ms: number | null;
  challenge_delay_ms: number;
  friends: string[];
  now_ms: number;
  key_is_public: boolean;
  version: string;
}

export interface Envelope {
  version: number;
  network: string;
  contract: string;
  mpc_contract: string;
  domain_id: number;
  derivation_path: string;
  mpc_public_key: string;
  algorithm: string;
  filename: string;
  nonce: string;
  ciphertext: string;
}

export function accountId(value: string): string {
  const account = value.trim();
  if (account.length < 2 || account.length > 64 ||
      !/^[a-z0-9]+(?:[._-][a-z0-9]+)*$/.test(account) || !account.endsWith('.testnet')) {
    throw new Error('Use a valid named .testnet account');
  }
  return account;
}

function object(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('Invalid response from NEAR');
  return value as Record<string, unknown>;
}

function millis(value: unknown): number {
  if (typeof value !== 'number' || !Number.isSafeInteger(value) || value < 0) {
    throw new Error('Invalid switch timestamp or response window');
  }
  return value;
}

const reader = new Near({ network: 'testnet', rpcUrl: RPC_URL, defaultWaitUntil: 'FINAL' });
let connector: NearConnector | undefined;
let writer: Near | undefined;
let started: Promise<void> | undefined;
let selectedAccount: string | null = null;
let walletRevision = 0;
const listeners = new Set<(account: string | null) => void>();
let submission: { account: string; onHash?: HashCallback; hash?: string } | undefined;

function publish(account: string | null) {
  selectedAccount = account;
  for (const listener of listeners) listener(account);
}

function selected(accounts: Array<{ accountId: string }>): string {
  if (accounts.length !== 1) throw new Error('Select one testnet account in Meteor, then reconnect');
  return accountId(accounts[0]!.accountId);
}

export function subscribeWallet(listener: (account: string | null) => void): () => void {
  listeners.add(listener);
  listener(selectedAccount);
  return () => { listeners.delete(listener); };
}

export function startWallet(): Promise<void> {
  if (started) return started;
  connector = new NearConnector({ network: 'testnet', autoConnect: false, providers: { testnet: [RPC_URL] } });
  connector.on('wallet:signIn', event => {
    walletRevision++;
    try { publish(event.success && event.wallet.manifest.id === 'meteor-wallet' ? selected(event.accounts) : null); }
    catch { publish(null); }
  });
  connector.on('wallet:signOut', () => { walletRevision++; publish(null); });
  const adapter = fromNearConnect(connector);
  writer = new Near({
    network: 'testnet', rpcUrl: RPC_URL, defaultWaitUntil: 'FINAL',
    wallet: {
      ...adapter,
      async signAndSendTransaction(params: Parameters<typeof adapter.signAndSendTransaction>[0]) {
        const account = await connectedAccount();
        if (!submission || account !== submission.account || params.signerId !== account) {
          throw new Error('The selected Meteor account changed. Review your account and try again');
        }
        const result = await adapter.signAndSendTransaction(params);
        // Keep the submitted hash even when the SDK's finality lookup fails.
        if (result.transaction?.hash) {
          submission.hash = result.transaction.hash;
          submission.onHash?.(result.transaction.hash);
        }
        return result;
      },
    },
  });
  const revision = walletRevision;
  started = connector.getConnectedWallet().then(({ wallet, accounts }) => {
    if (revision === walletRevision) publish(wallet.manifest.id === 'meteor-wallet' ? selected(accounts) : null);
  }).catch(() => { if (revision === walletRevision) publish(null); });
  return started;
}

export async function connect(): Promise<void> {
  await startWallet();
  walletRevision++;
  const wallet = await connector!.connect({ walletId: 'meteor-wallet' });
  publish(selected(await wallet.getAccounts()));
}

export async function disconnect(): Promise<void> {
  if (submission) throw new Error('Wait for the current transaction before disconnecting');
  await startWallet();
  walletRevision++;
  await connector!.disconnect();
  publish(null);
}

async function connectedAccount(): Promise<string> {
  await startWallet();
  if (!selectedAccount) throw new Error('Connect a testnet account with Meteor first');
  const { wallet, accounts } = await connector!.getConnectedWallet();
  if (wallet.manifest.id !== 'meteor-wallet') throw new Error('Connect with Meteor to continue');
  const actual = selected(accounts);
  if (actual !== selectedAccount) {
    publish(actual);
    throw new Error('The selected Meteor account changed. Review your account and try again');
  }
  return actual;
}

export async function getStatus(value: string): Promise<SwitchStatus> {
  const account = accountId(value);
  let response: unknown;
  try {
    response = await reader.view(account, 'get_status', {}, { finality: 'final' });
  } catch (error) {
    const missingCode = error instanceof ContractNotDeployedError && error.accountId === account;
    // call_function currently reports missing WASM code in FunctionCallError.panic.
    const missingViewCode = error instanceof FunctionCallError && error.contractId === account &&
      error.methodName === 'get_status' && error.panic?.includes('CompilationError(CodeDoesNotExist {');
    if (missingCode || missingViewCode) {
      throw new Error('This account has no switch yet. Set it up, or look up another switch account.', { cause: error });
    }
    throw error;
  }
  const status = object(response);
  if (status.version !== '1' || status.owner !== account || !Array.isArray(status.friends) ||
      !status.friends.every(friend => typeof friend === 'string') || typeof status.key_is_public !== 'boolean') {
    throw new Error('Unsupported switch contract. It must include the updated get_status API');
  }
  const deadline = status.challenge_deadline_ms === null ? null : millis(status.challenge_deadline_ms);
  const now = millis(status.now_ms);
  const delay = millis(status.challenge_delay_ms);
  if (delay === 0 || status.key_is_public !== (deadline !== null && now >= deadline)) {
    throw new Error('Inconsistent switch release status');
  }
  return { owner: account, version: '1', friends: status.friends.map(friend => accountId(friend as string)),
    challenge_deadline_ms: deadline, challenge_delay_ms: delay, now_ms: now, key_is_public: status.key_is_public };
}

async function submit(account: string, build: (near: Near) => TransactionBuilder, onHash?: HashCallback): Promise<FinalExecutionOutcome> {
  if (submission) throw new Error('A transaction is already in progress');
  submission = { account, onHash };
  try {
    const result = await build(writer!).send({ waitUntil: 'FINAL' });
    if (result.transaction.signer_id !== account) {
      throw new Error('Meteor submitted this transaction from a different account. Check the transaction before continuing');
    }
    if (typeof result.status !== 'object' || !('SuccessValue' in result.status)) {
      throw new Error('The transaction did not complete successfully');
    }
    return result;
  } catch (error) {
    const hash = submission.hash;
    if (hash) throw new Error(`${error instanceof Error ? error.message : String(error)}. Check transaction ${hash} before retrying`, { cause: error });
    throw error;
  } finally { submission = undefined; }
}

export async function setupSwitch(global: string, delayMs: number, friends: string[], expectedAccount: string, onHash?: HashCallback): Promise<void> {
  const publisher = accountId(global);
  if (millis(delayMs) === 0) throw new Error('Choose a positive response window');
  const trusted = friends.map(accountId);
  const account = await connectedAccount();
  if (account !== accountId(expectedAccount)) throw new Error('The selected Meteor account changed. Review setup for this account before continuing');
  const state = await reader.getAccount(account, { finality: 'final' });
  if (state.hasContract) throw new Error('This account already has a contract. Use a fresh dedicated account in Meteor');
  await submit(account, near => near.transaction(account).deployFromPublished({ accountId: publisher })
    .functionCall(account, 'init', { challenge_delay_ms: delayMs, friends: trusted }, { gas: '150 Tgas' }), onHash);
  await getStatus(account);
}

export async function callSwitch(value: string, method: string, args: object, onHash?: HashCallback): Promise<void> {
  const contract = accountId(value);
  const account = await connectedAccount();
  await getStatus(contract);
  await submit(account, near => near.transaction(account).functionCall(contract, method, args, { gas: '150 Tgas' }), onHash);
}

export async function deriveKey(value: string, onHash?: HashCallback): Promise<{ key: CapsuleKey; mpcKey: string }> {
  const contract = accountId(value);
  const account = await connectedAccount();
  const status = await getStatus(contract);
  if (status.owner !== account && !status.key_is_public) throw new Error('The response window has not expired');
  const state = object(await reader.view(MPC_CONTRACT, 'state', {}, { finality: 'final' }));
  const domains = object(object(state.Running).keyset).domains;
  if (!Array.isArray(domains)) throw new Error('MPC is not running');
  const domain = domains.find(value => object(value).domain_id === DOMAIN_ID);
  const mpcKey = object(object(object(domain).key).Bls12381).public_key;
  if (typeof mpcKey !== 'string') throw new Error('MPC CKD domain 2 is unavailable');
  const { CkdRequest } = await loadCrypto();
  const request = new CkdRequest();
  try {
    const result = await submit(account, near => near.transaction(account).functionCall(contract, 'request_confidential_key',
      { app_public_key: { AppPublicKey: request.publicKey() } }, { gas: '150 Tgas' }), onHash);
    if (typeof result.status !== 'object' || !('SuccessValue' in result.status)) throw new Error('MPC did not return a CKD response');
    const bytes = Uint8Array.from(atob(result.status.SuccessValue), character => character.charCodeAt(0));
    const response = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
    return { key: request.derive(response, contract, mpcKey), mpcKey };
  } finally { request.free(); }
}
