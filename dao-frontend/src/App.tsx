import React, { useEffect, useState } from 'react';
import { setupWalletSelector } from '@near-wallet-selector/core';
import { setupMyNearWallet } from '@near-wallet-selector/my-near-wallet';
import { setupModal } from '@near-wallet-selector/modal-ui';
import type { WalletSelector } from '@near-wallet-selector/core';
import '@near-wallet-selector/modal-ui/styles.css';
import './App.css';

import { JoinDAO } from './components/JoinDAO';
import { CreateProposal } from './components/CreateProposal';
import { ProposalList } from './components/ProposalList';
import { VoteOnProposal } from './components/VoteOnProposal';
import { DAOInfo } from './types';

function App() {
  const [selector, setSelector] = useState<WalletSelector | null>(null);
  const [modal, setModal] = useState<any>(null);
  const [accountId, setAccountId] = useState<string | null>(null);
  const [daoInfo, setDAOInfo] = useState<DAOInfo | null>(null);
  const [isMember, setIsMember] = useState<boolean>(false);
  const [hasPubkey, setHasPubkey] = useState<boolean>(false);
  const [activeTab, setActiveTab] = useState<'join' | 'proposals' | 'create' | 'vote'>('join');

  const contractId = process.env.REACT_APP_CONTRACT_ID || 'privatedao.testnet';
  const network = process.env.REACT_APP_NEAR_NETWORK || 'testnet';
  const rpcUrl = process.env.REACT_APP_NEAR_RPC_URL || (
    network === 'testnet'
      ? 'https://rpc.testnet.near.org'
      : 'https://rpc.mainnet.near.org'
  );

  // Helper function to call view methods via RPC
  const viewMethod = async (method: string, args: any = {}) => {
    const response = await fetch(rpcUrl, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        jsonrpc: '2.0',
        id: 'dontcare',
        method: 'query',
        params: {
          request_type: 'call_function',
          finality: 'final',
          account_id: contractId,
          method_name: method,
          args_base64: btoa(JSON.stringify(args)),
        },
      }),
    });

    const data = await response.json();

    if (data.error) {
      throw new Error(data.error.message || 'View method call failed');
    }

    const resultBytes = data.result?.result;
    if (!resultBytes || resultBytes.length === 0) {
      return null;
    }

    const resultStr = new TextDecoder().decode(new Uint8Array(resultBytes));
    return JSON.parse(resultStr);
  };

  // Initialize wallet on mount
  useEffect(() => {
    initWallet();
  }, []);

  // Fetch DAO info when wallet connects
  useEffect(() => {
    if (selector && accountId) {
      fetchDAOInfo();
      checkMembership();
    }
  }, [selector, accountId]);

  const initWallet = async () => {
    const _selector = await setupWalletSelector({
      network: network as 'testnet' | 'mainnet',
      modules: [setupMyNearWallet()],
    });

    const _modal = setupModal(_selector, {
      contractId: contractId,
    });

    setSelector(_selector);
    setModal(_modal);

    // Check if already signed in
    const state = _selector.store.getState();
    if (state.accounts.length > 0) {
      setAccountId(state.accounts[0].accountId);
    }

    // Subscribe to account changes
    _selector.store.observable.subscribe((state) => {
      if (state.accounts.length > 0) {
        setAccountId(state.accounts[0].accountId);
      } else {
        setAccountId(null);
        setIsMember(false);
        setHasPubkey(false);
      }
    });
  };

  const fetchDAOInfo = async () => {
    try {
      const info = await viewMethod('get_dao_info', {});
      setDAOInfo(info as DAOInfo);
    } catch (error) {
      console.error('Failed to fetch DAO info:', error);
    }
  };

  const checkMembership = async () => {
    if (!accountId) return;

    try {
      // Check if member
      const member = await viewMethod('is_member', { account_id: accountId });
      setIsMember(member as boolean);

      if (member) {
        // Check if has pubkey
        const pubkey = await viewMethod('get_user_pubkey', { account_id: accountId });
        setHasPubkey(pubkey !== null);
      }
    } catch (error) {
      console.error('Failed to check membership:', error);
    }
  };

  const handleSignIn = () => {
    if (modal) {
      modal.show();
    }
  };

  const handleSignOut = async () => {
    if (selector) {
      const wallet = await selector.wallet();
      await wallet.signOut();
      setAccountId(null);
      setIsMember(false);
      setHasPubkey(false);
    }
  };

  const refreshData = () => {
    fetchDAOInfo();
    checkMembership();
  };

  return (
    <div className="App">
      <header className="App-header">
        <div className="header-content">
          <h1>🗳️ Private DAO</h1>
          {daoInfo && (
            <div className="dao-info">
              <h2>{daoInfo.name}</h2>
              <p>Mode: {daoInfo.membership_mode}</p>
              <p>Members: {daoInfo.member_count}</p>
            </div>
          )}
        </div>
        <div className="wallet-section">
          {accountId ? (
            <div className="wallet-connected">
              <span className="account-badge">{accountId}</span>
              {isMember && <span className="member-badge">✓ Member</span>}
              {hasPubkey && <span className="key-badge">🔑 Key Ready</span>}
              <button onClick={handleSignOut} className="btn-secondary">
                Sign Out
              </button>
            </div>
          ) : (
            <button onClick={handleSignIn} className="btn-primary">
              Connect Wallet
            </button>
          )}
        </div>
      </header>

      {accountId ? (
        <main className="main-content">
          <nav className="tabs">
            <button
              className={activeTab === 'join' ? 'tab active' : 'tab'}
              onClick={() => setActiveTab('join')}
            >
              Join DAO
            </button>
            <button
              className={activeTab === 'proposals' ? 'tab active' : 'tab'}
              onClick={() => setActiveTab('proposals')}
              disabled={!isMember || !hasPubkey}
            >
              Proposals
            </button>
            <button
              className={activeTab === 'create' ? 'tab active' : 'tab'}
              onClick={() => setActiveTab('create')}
              disabled={!isMember || !hasPubkey}
            >
              Create Proposal
            </button>
            <button
              className={activeTab === 'vote' ? 'tab active' : 'tab'}
              onClick={() => setActiveTab('vote')}
              disabled={!isMember || !hasPubkey}
            >
              Vote
            </button>
          </nav>

          <div className="tab-content">
            {activeTab === 'join' && (
              <JoinDAO
                selector={selector}
                accountId={accountId}
                contractId={contractId}
                isMember={isMember}
                hasPubkey={hasPubkey}
                membershipMode={daoInfo?.membership_mode || 'Public'}
                onSuccess={refreshData}
              />
            )}

            {activeTab === 'proposals' && (
              <ProposalList
                selector={selector}
                accountId={accountId}
                contractId={contractId}
                network={network}
                viewMethod={viewMethod}
              />
            )}

            {activeTab === 'create' && (
              <CreateProposal
                selector={selector}
                accountId={accountId}
                contractId={contractId}
                onSuccess={() => setActiveTab('proposals')}
                viewMethod={viewMethod}
              />
            )}

            {activeTab === 'vote' && (
              <VoteOnProposal
                selector={selector}
                accountId={accountId}
                contractId={contractId}
                network={network}
                onSuccess={refreshData}
                viewMethod={viewMethod}
              />
            )}
          </div>
        </main>
      ) : (
        <main className="main-content">
          <div className="welcome-screen">
            <h2>Welcome to Private DAO</h2>
            <p>Anonymous voting powered by NEAR OutLayer and TEE</p>
            <p>Connect your wallet to get started</p>
          </div>
        </main>
      )}

      <footer className="App-footer">
        <p>Powered by NEAR OutLayer • Private voting with TEE</p>
        <a
          href="https://github.com/out-layer/private-dao-example"
          target="_blank"
          rel="noopener noreferrer"
        >
          View on GitHub
        </a>
      </footer>
    </div>
  );
}

export default App;
