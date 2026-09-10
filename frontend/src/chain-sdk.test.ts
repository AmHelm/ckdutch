import { afterEach, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({ on: vi.fn(), sign: vi.fn() }));
const accounts = [{ accountId: 'alice.testnet' }, { accountId: 'bob.testnet' }];

vi.mock('@hot-labs/near-connect', () => ({ NearConnector: class {
  on = mocks.on;
  async wallet() {
    return { manifest: { id: 'meteor-wallet' }, getAccounts: async () => accounts,
      signAndSendTransaction: mocks.sign };
  }
  async getConnectedWallet() { return { wallet: await this.wallet(), accounts }; }
  async connect() {
    const wallet = await this.wallet();
    mocks.on.mock.calls.find(([event]) => event === 'wallet:signIn')![1]({
      wallet, accounts: [accounts[1]], success: true,
    });
    return wallet;
  }
} }));

afterEach(() => vi.unstubAllGlobals());

it('uses the explicit selected signer through the published near-kit SDK when it is not first', async () => {
  const status = { owner: 'bob.testnet', version: '1', friends: [],
    challenge_deadline_ms: null, challenge_delay_ms: 60_000, now_ms: 1000, key_is_public: false };
  vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response(JSON.stringify({
    jsonrpc: '2.0', id: 'test', result: {
      result: Array.from(new TextEncoder().encode(JSON.stringify(status))), logs: [],
      block_height: 1, block_hash: '11111111111111111111111111111111',
    },
  }))));
  mocks.sign.mockResolvedValue({ transaction: { hash: 'submitted-hash', signer_id: 'bob.testnet' },
    status: { SuccessValue: '' }, final_execution_status: 'FINAL' });
  const chain = await import('./chain');
  await chain.connect('meteor-wallet');
  await chain.callSwitch('bob.testnet', 'claim_owner_is_alive', {});
  expect(mocks.sign).toHaveBeenCalledExactlyOnceWith({
    signerId: 'bob.testnet', receiverId: 'bob.testnet',
    actions: [{ type: 'FunctionCall', params: {
      methodName: 'claim_owner_is_alive', args: {}, gas: '150000000000000', deposit: '0',
    } }],
  });
});
