import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  getConnectedWallet: vi.fn(), connect: vi.fn(), disconnect: vi.fn(), on: vi.fn(),
  view: vi.fn(), getAccount: vi.fn(), sign: vi.fn(), send: vi.fn(), settle: vi.fn(),
  deploy: vi.fn(), call: vi.fn(), publicKey: vi.fn(), derive: vi.fn(), free: vi.fn(),
}));

vi.mock('@hot-labs/near-connect', () => ({ NearConnector: class {
  getConnectedWallet = mocks.getConnectedWallet;
  connect = mocks.connect;
  disconnect = mocks.disconnect;
  on = mocks.on;
} }));

vi.mock('near-kit', async importOriginal => ({
  ...await importOriginal<typeof import('near-kit')>(),
  fromNearConnect: () => ({ signAndSendTransaction: mocks.sign }),
  Near: class {
    constructor(private options: { wallet?: { signAndSendTransaction: (params: object) => Promise<unknown> } }) {}
    view = mocks.view;
    getAccount = mocks.getAccount;
    transaction(account: string) {
      const wallet = this.options.wallet!;
      const builder = {
        deployFromPublished(reference: object) { mocks.deploy(reference); return builder; },
        functionCall(...args: unknown[]) { mocks.call(...args); return builder; },
        async send(options: object) {
          mocks.send(options);
          return mocks.settle(await wallet.signAndSendTransaction({ signerId: account }));
        },
      };
      return builder;
    }
  },
}));

vi.mock('./crypto', () => ({ loadCrypto: async () => ({ CkdRequest: class {
  publicKey = mocks.publicKey;
  derive = mocks.derive;
  free = mocks.free;
} }) }));

const status = {
  owner: 'alice.testnet', version: '1', friends: ['friend.testnet'],
  challenge_deadline_ms: null, challenge_delay_ms: 60_000, now_ms: 1000, key_is_public: false,
};

function wallet(account = 'alice.testnet') {
  return { wallet: { manifest: { id: 'meteor-wallet' } }, accounts: [{ accountId: account }] };
}

function outcome(account = 'alice.testnet') {
  return { transaction: { hash: 'submitted-hash', signer_id: account }, status: { SuccessValue: btoa('{}') }, final_execution_status: 'FINAL' };
}

beforeEach(() => {
  vi.resetModules();
  vi.resetAllMocks();
  mocks.getConnectedWallet.mockResolvedValue(wallet());
  mocks.view.mockResolvedValue(status);
  mocks.getAccount.mockResolvedValue({ hasContract: false });
  mocks.sign.mockResolvedValue(outcome());
  mocks.settle.mockImplementation(value => value);
  mocks.publicKey.mockReturnValue('bls12381g1:ephemeral');
});

describe('wallet and contract boundaries', () => {
  it('restores only after startup and delivers the restored account to existing subscribers', async () => {
    const chain = await import('./chain');
    const listener = vi.fn();
    const unsubscribe = chain.subscribeWallet(listener);
    expect(mocks.getConnectedWallet).not.toHaveBeenCalled();
    expect(listener).toHaveBeenLastCalledWith(null);
    await chain.startWallet();
    expect(listener).toHaveBeenLastCalledWith('alice.testnet');
    unsubscribe();
  });

  it('rejects malformed and non-testnet account IDs', async () => {
    const { accountId } = await import('./chain');
    expect(accountId(' alice.testnet ')).toBe('alice.testnet');
    for (const account of ['alice.near', 'Alice.testnet', 'alice..testnet', 'alice-.testnet', 'x'.repeat(60) + '.testnet']) {
      expect(() => accountId(account)).toThrow();
    }
  });

  it('connects Meteor without requesting an application access key or message signature', async () => {
    const chain = await import('./chain');
    mocks.connect.mockResolvedValue({ getAccounts: async () => [{ accountId: 'alice.testnet' }] });
    await chain.connect();
    expect(mocks.connect).toHaveBeenCalledWith({ walletId: 'meteor-wallet' });
    expect(mocks.sign).not.toHaveBeenCalled();
  });

  it('rejects malformed or inconsistent public-release status', async () => {
    const { getStatus } = await import('./chain');
    for (const changed of [{ owner: 'other.testnet' }, { now_ms: Number.MAX_SAFE_INTEGER + 1 },
      { challenge_deadline_ms: undefined }, { key_is_public: true }, { friends: [7] }]) {
      mocks.view.mockResolvedValue({ ...status, ...changed });
      await expect(getStatus('alice.testnet')).rejects.toThrow();
    }
    expect(mocks.view).toHaveBeenCalledWith('alice.testnet', 'get_status', {}, { finality: 'final' });
  });

  it('explains missing switch code without concealing RPC or other contract failures', async () => {
    const { getStatus } = await import('./chain');
    const { ContractNotDeployedError, FunctionCallError } = await import('near-kit');
    for (const error of [new ContractNotDeployedError('alice.testnet'),
      new FunctionCallError('alice.testnet', 'get_status',
        'wasm execution failed with error: CompilationError(CodeDoesNotExist { account_id: "alice.testnet" })')]) {
      mocks.view.mockRejectedValue(error);
      await expect(getStatus('alice.testnet')).rejects.toThrow('This account has no switch yet. Set it up, or look up another switch account.');
    }
    for (const error of [new Error('RPC network timeout'), new FunctionCallError('alice.testnet', 'get_status', 'Unrelated contract failure')]) {
      mocks.view.mockRejectedValue(error);
      await expect(getStatus('alice.testnet')).rejects.toBe(error);
    }
  });

  it('refuses to overwrite an existing contract before asking for a signature', async () => {
    const chain = await import('./chain');
    mocks.getAccount.mockResolvedValue({ hasContract: true });
    await expect(chain.setupSwitch('publisher.testnet', 60_000, [], 'alice.testnet')).rejects.toThrow('already has a contract');
    expect(mocks.sign).not.toHaveBeenCalled();
  });

  it('attaches and initializes in one finalized transaction', async () => {
    const chain = await import('./chain');
    const onHash = vi.fn();
    await chain.setupSwitch('publisher.testnet', 60_000, ['friend.testnet'], 'alice.testnet', onHash);
    expect(mocks.deploy).toHaveBeenCalledWith({ accountId: 'publisher.testnet' });
    expect(mocks.call).toHaveBeenCalledWith('alice.testnet', 'init',
      { challenge_delay_ms: 60_000, friends: ['friend.testnet'] }, { gas: '150 Tgas' });
    expect(mocks.send).toHaveBeenCalledOnce();
    expect(mocks.send).toHaveBeenCalledWith({ waitUntil: 'FINAL' });
    expect(onHash).toHaveBeenCalledWith('submitted-hash');
  });

  it('binds setup consent to the account shown before the operation started', async () => {
    const chain = await import('./chain');
    mocks.getConnectedWallet.mockResolvedValue(wallet('bob.testnet'));
    await expect(chain.setupSwitch('publisher.testnet', 60_000, [], 'alice.testnet')).rejects.toThrow('account changed');
    expect(mocks.getAccount).not.toHaveBeenCalled();
    expect(mocks.sign).not.toHaveBeenCalled();
  });

  it('rechecks Meteor account after preflight and aborts if it changed', async () => {
    const chain = await import('./chain');
    await chain.startWallet();
    mocks.getConnectedWallet.mockResolvedValueOnce(wallet()).mockResolvedValue(wallet('bob.testnet'));
    await expect(chain.callSwitch('alice.testnet', 'claim_owner_is_alive', {})).rejects.toThrow('account changed');
    expect(mocks.sign).not.toHaveBeenCalled();
  });

  it('reports a returned signer mismatch with the submitted hash without resubmitting', async () => {
    const chain = await import('./chain');
    const onHash = vi.fn();
    mocks.sign.mockResolvedValue(outcome('bob.testnet'));
    await expect(chain.callSwitch('alice.testnet', 'claim_owner_is_alive', {}, onHash))
      .rejects.toThrow('Check transaction submitted-hash before retrying');
    expect(onHash).toHaveBeenCalledWith('submitted-hash');
    expect(mocks.sign).toHaveBeenCalledOnce();
  });

  it('retains the submitted hash when finality reconciliation fails', async () => {
    const chain = await import('./chain');
    const onHash = vi.fn();
    mocks.settle.mockRejectedValue(new Error('RPC timeout'));
    await expect(chain.callSwitch('alice.testnet', 'claim_owner_is_alive', {}, onHash))
      .rejects.toThrow('RPC timeout. Check transaction submitted-hash before retrying');
    expect(onHash).toHaveBeenCalledWith('submitted-hash');
    expect(mocks.sign).toHaveBeenCalledOnce();
  });

  it('does not request a protected key for a trusted friend', async () => {
    const chain = await import('./chain');
    mocks.getConnectedWallet.mockResolvedValue(wallet('friend.testnet'));
    await expect(chain.deriveKey('alice.testnet')).rejects.toThrow('response window has not expired');
    expect(mocks.sign).not.toHaveBeenCalled();
  });

  it('frees the ephemeral request even when MPC response verification fails', async () => {
    const chain = await import('./chain');
    mocks.view.mockResolvedValueOnce(status).mockResolvedValueOnce({
      Running: { keyset: { domains: [{ domain_id: 2, key: { Bls12381: { public_key: 'mpc-key' } } }] } },
    });
    mocks.derive.mockImplementation(() => { throw new Error('Invalid MPC proof'); });
    await expect(chain.deriveKey('alice.testnet')).rejects.toThrow('Invalid MPC proof');
    expect(mocks.call).toHaveBeenCalledWith('alice.testnet', 'request_confidential_key',
      { app_public_key: { AppPublicKey: 'bls12381g1:ephemeral' } }, { gas: '150 Tgas' });
    expect(mocks.free).toHaveBeenCalledOnce();
  });
});
