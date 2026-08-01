/*!
 * Private DAO Smart Contract
 *
 * Features:
 * - Public or Private membership modes
 * - Anonymous voting with encrypted ballots
 * - OutLayer integration for key derivation and vote tallying
 * - User pays for OutLayer execution
 * - Proposal creation and voting
 * - TEE-based vote verification
 */

mod types;

use near_sdk::borsh::{BorshDeserialize, BorshSerialize};
use near_sdk::collections::{LookupMap, UnorderedMap, Vector};
use near_sdk::{
    env, ext_contract, log, near_bindgen, AccountId, Gas, NearToken, Promise, BorshStorageKey,
    PromiseError, PanicOnDefault,
};

type Balance = u128;

use types::*;

/// Minimum deposit for OutLayer execution (0.01 NEAR)
const MIN_OUTLAYER_DEPOSIT: Balance = 10_000_000_000_000_000_000_000;

/// Storage deposit per member (for public key storage)
const STORAGE_DEPOSIT_PER_MEMBER: Balance = 2_000_000_000_000_000_000_000; // 0.002 NEAR

/// Storage deposit per vote
const STORAGE_DEPOSIT_PER_VOTE: Balance = 2_000_000_000_000_000_000_000; // 0.002 NEAR

/// Gas for callback
const CALLBACK_GAS: Gas = Gas::from_tgas(10);

/// OutLayer contract ID
const OUTLAYER_CONTRACT_ID: &str = "outlayer.testnet";

/// External contract interface for OutLayer
#[ext_contract(ext_outlayer)]
#[allow(dead_code)]
trait OutLayer {
    fn request_execution(
        &mut self,
        source: serde_json::Value,
        resource_limits: Option<serde_json::Value>,
        input_data: Option<String>,
        secrets_ref: Option<serde_json::Value>,
        response_format: Option<String>,
        payer_account_id: Option<AccountId>,
    );
}

/// External contract interface for self callbacks
#[ext_contract(ext_self)]
#[allow(dead_code)]
trait ExtSelf {
    fn on_key_derived(
        &mut self,
        user: AccountId,
        #[callback_result] result: Result<Option<DeriveKeyResponse>, PromiseError>,
    );

    fn on_votes_tallied(
        &mut self,
        proposal_id: u64,
        #[callback_result] result: Result<Option<TallyResponse>, PromiseError>,
    );
}

#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize, PanicOnDefault)]
#[borsh(crate = "near_sdk::borsh")]
pub struct PrivateDAO {
    /// DAO owner (admin)
    pub owner: AccountId,

    /// DAO name
    pub name: String,

    /// Membership mode: Public or Private
    pub membership_mode: MembershipMode,

    /// Members list (account_id → MemberInfo with joined_at timestamp)
    pub members: LookupMap<AccountId, MemberInfo>,

    /// Member count (for public display)
    pub member_count: u64,

    /// User public keys (account_id → public_key_hex)
    /// Public keys are used for client-side encryption
    pub user_pubkeys: LookupMap<AccountId, String>,

    /// Proposals (proposal_id → Proposal)
    pub proposals: UnorderedMap<u64, Proposal>,

    /// Next proposal ID
    pub next_proposal_id: u64,

    /// Votes (proposal_id → Vector<Vote>)
    pub votes: LookupMap<u64, Vector<Vote>>,
}

#[derive(BorshSerialize, BorshStorageKey)]
#[borsh(crate = "near_sdk::borsh")]
enum StorageKey {
    Members,
    UserPubKeys,
    Proposals,
    Votes
}

#[near_bindgen]
impl PrivateDAO {
    /// Initialize a new DAO
    ///
    /// # Arguments
    /// * `name` - DAO name
    /// * `membership_mode` - Public or Private membership
    /// * `owner` - DAO owner/admin account
    #[init]
    pub fn new(name: String, membership_mode: MembershipMode, owner: AccountId) -> Self {
        assert!(!env::state_exists(), "Already initialized");

        let mut dao = Self {
            owner: owner.clone(),
            name,
            membership_mode,
            members: LookupMap::new(StorageKey::Members),
            member_count: 0,
            user_pubkeys: LookupMap::new(StorageKey::UserPubKeys),
            proposals: UnorderedMap::new(StorageKey::Proposals),
            next_proposal_id: 1,
            votes: LookupMap::new(StorageKey::Votes),
        };

        // Add owner as first member
        dao.members.insert(&owner, &MemberInfo {
            joined_at: env::block_timestamp(),
        });
        dao.member_count = 1;

        dao
    }

    /// Join the DAO and derive encryption public key
    ///
    /// In Public mode: anyone can join
    /// In Private mode: only invited members can join
    ///
    /// # Payment
    /// Requires:
    /// - 0.002 NEAR for storage deposit (member record)
    /// - 0.01 NEAR for OutLayer execution (key derivation)
    /// Total: ~0.012 NEAR
    ///
    /// # Process
    /// 1. User pays and joins DAO
    /// 2. Contract calls OutLayer to derive user's public key
    /// 3. OutLayer callback stores public key on contract
    /// 4. User can now vote with encrypted ballots
    #[payable]
    pub fn join_dao(&mut self) -> Promise {
        let user = env::predecessor_account_id();
        let attached = env::attached_deposit();

        // Check if already a member
        if self.members.get(&user).is_some() {
            env::panic_str("Already a member");
        }

        // In Private mode, user must be pre-approved (added by owner)
        if self.membership_mode == MembershipMode::Private {
            env::panic_str("Private DAO: join by invitation only");
        }

        // Check deposit covers storage + OutLayer execution
        let required_deposit = STORAGE_DEPOSIT_PER_MEMBER + MIN_OUTLAYER_DEPOSIT;
        assert!(
            attached.as_yoctonear() >= required_deposit,
            "Minimum deposit is {} yoctoNEAR (storage + OutLayer execution)",
            required_deposit
        );

        log!("User {} requesting to join DAO. Deriving encryption public key via OutLayer", user);

        // NOTE: User is NOT added to members yet
        // They will be added in on_key_derived callback after successful key generation
        // This ensures atomicity: user is only a member if they have a valid pubkey

        // Call OutLayer to derive user's public key
        self.request_key_derivation(user.clone(), attached.as_yoctonear())
    }

    /// Add member (Private DAO only, owner-only)
    ///
    /// In Private mode, owner pre-approves members before they can join
    pub fn add_member(&mut self, account_id: AccountId) {
        self.assert_owner();

        if self.membership_mode == MembershipMode::Public {
            env::panic_str("Public DAO: users join directly");
        }

        if self.members.get(&account_id).is_some() {
            env::panic_str("Already a member");
        }

        self.members.insert(&account_id, &MemberInfo {
            joined_at: env::block_timestamp(),
        });
        self.member_count += 1;

        log!("Added {} to private DAO (pre-approved)", account_id);
    }

    /// Leave the DAO (self-removal)
    ///
    /// Any member can leave the DAO at any time.
    ///
    /// Removes:
    /// - Member status
    /// - Public key (if exists)
    /// - Does NOT remove votes (preserves historical data)
    ///
    /// Note: No refund is provided (storage deposit is forfeited)
    pub fn leave_dao(&mut self) {
        let user = env::predecessor_account_id();

        // Check if member exists
        if self.members.get(&user).is_none() {
            env::panic_str("Not a member");
        }

        // Remove from members
        self.members.remove(&user);
        self.member_count -= 1;

        // Remove pubkey if exists
        if self.user_pubkeys.get(&user).is_some() {
            self.user_pubkeys.remove(&user);
        }

        log!("User {} left the DAO", user);
    }

    /// Remove member (owner-only)
    ///
    /// **FOR TESTING ONLY**: Removes a member from the DAO.
    /// In production, this should require governance vote or more strict conditions.
    ///
    /// Removes:
    /// - Member status
    /// - Public key (if exists)
    /// - Does NOT remove votes (preserves historical data)
    pub fn remove_member(&mut self, account_id: AccountId) {
        self.assert_owner();

        // Check if member exists
        if self.members.get(&account_id).is_none() {
            env::panic_str("Not a member");
        }

        // Remove from members
        self.members.remove(&account_id);
        self.member_count -= 1;

        // Remove pubkey if exists
        if self.user_pubkeys.get(&account_id).is_some() {
            self.user_pubkeys.remove(&account_id);
        }

        log!("TESTING: Owner removed {} from DAO", account_id);
    }

    /// Manually add member with timestamp (owner-only, for migration)
    ///
    /// This is a migration helper to add members with joined_at = 0 after deploying V2.
    /// Use this to restore members who were in the old contract.
    ///
    /// joined_at = 0 means they can vote on all proposals (old and new)
    pub fn migrate_add_member(&mut self, account_id: AccountId, pubkey: Option<String>) {
        self.assert_owner();

        // Add member with joined_at = 0 (can vote on everything)
        self.members.insert(&account_id, &MemberInfo { joined_at: 0 });

        // Add pubkey if provided
        if let Some(pk) = pubkey {
            self.user_pubkeys.insert(&account_id, &pk);
        }

        self.member_count += 1;

        log!(
            "Migration: Added member {} with joined_at=0 (can vote on all proposals)",
            account_id
        );
    }

    /// Reset state (TESTING ONLY - clears everything)
    ///
    /// **FOR TESTING ONLY**: Recreates all storage collections with new format.
    /// Use this after contract upgrade when state format changed.
    ///
    /// WARNING: This will clear all proposals and votes!
    /// Members and pubkeys are preserved.
    ///
    /// NOTE: Must be called by contract account itself (use near-cli with --accountId same as contract)
    #[init(ignore_state)]
    pub fn reset_state() -> Self {
        // Get old state
        let old_state: PrivateDAO = env::state_read().expect("Failed to read state");

        // Verify caller is owner
        assert_eq!(
            env::predecessor_account_id(),
            old_state.owner,
            "Only owner can migrate"
        );

        // Create new state - keep members/pubkeys as-is, recreate proposals/votes
        let new_state = Self {
            owner: old_state.owner.clone(),
            name: old_state.name.clone(),
            membership_mode: old_state.membership_mode.clone(),
            members: LookupMap::new(StorageKey::Members),
            member_count: 0,
            user_pubkeys: LookupMap::new(StorageKey::UserPubKeys),
            proposals: UnorderedMap::new(StorageKey::Proposals),
            next_proposal_id: 1,
            votes: LookupMap::new(StorageKey::Votes),
        };

        log!(
            "MIGRATION: State migrated. Members: {}, Proposals cleared",
            new_state.member_count
        );

        new_state
    }

    /// Complete join after pre-approval (Private DAO)
    ///
    /// After owner adds member, user must call this to derive their key
    #[payable]
    pub fn complete_join(&mut self) -> Promise {
        let user = env::predecessor_account_id();
        let attached = env::attached_deposit();

        // Check if pre-approved
        if self.members.get(&user).is_none() {
            env::panic_str("Not pre-approved. Contact DAO owner.");
        }

        // Check if already has pubkey (already completed join)
        if self.user_pubkeys.get(&user).is_some() {
            env::panic_str("Already completed join");
        }

        // Check deposit for OutLayer execution
        assert!(
            attached.as_yoctonear() >= MIN_OUTLAYER_DEPOSIT,
            "Minimum deposit is {} yoctoNEAR for OutLayer execution",
            MIN_OUTLAYER_DEPOSIT
        );

        log!("User {} completing join. Deriving encryption public key", user);

        // Call OutLayer to derive user's public key
        self.request_key_derivation(user.clone(), attached.as_yoctonear())
    }

    /// Create a new proposal
    ///
    /// # Arguments
    /// * `title` - Proposal title
    /// * `description` - Proposal description
    /// * `quorum` - Quorum requirements for passing
    /// * `deadline` - Optional voting deadline (nanoseconds since epoch). If None, no time limit.
    ///
    /// # Payment
    /// Requires 0.001 NEAR for storage
    #[payable]
    pub fn create_proposal(
        &mut self,
        title: String,
        description: String,
        quorum: QuorumType,
        deadline: Option<u64>,
    ) -> u64 {
        let creator = env::predecessor_account_id();

        // Only members can create proposals
        let member_info = self.members.get(&creator)
            .expect("Only members can create proposals");

        // Check storage deposit
        let attached = env::attached_deposit();
        assert!(
            attached.as_yoctonear() >= 1_000_000_000_000_000_000_000, // 0.001 NEAR
            "Minimum deposit is 0.001 NEAR for storage"
        );

        // Validate deadline is in the future (if provided)
        if let Some(deadline_ns) = deadline {
            assert!(
                deadline_ns > env::block_timestamp(),
                "Deadline must be in the future"
            );
        }

        // Validate creator joined before proposal creation (prevent retroactive voting)
        // This ensures members can only vote on proposals created AFTER they joined
        assert!(
            member_info.joined_at <= env::block_timestamp(),
            "Invalid member timestamp"
        );

        let proposal_id = self.next_proposal_id;
        self.next_proposal_id += 1;

        let proposal = Proposal {
            id: proposal_id,
            title,
            description,
            creator: creator.clone(),
            created_at: env::block_timestamp(),
            deadline,
            quorum,
            status: ProposalStatus::Active,
            tally_result: None,
        };

        self.proposals.insert(&proposal_id, &proposal);

        // Create unique storage key for this proposal's votes
        let votes_key = format!("v{}", proposal_id);
        self.votes.insert(&proposal_id, &Vector::new(votes_key.as_bytes()));

        log!(
            "Proposal {} created by {}: '{}'",
            proposal_id,
            creator,
            proposal.title
        );

        proposal_id
    }

    /// Cast a vote on a proposal
    ///
    /// # Arguments
    /// * `proposal_id` - Proposal ID
    /// * `encrypted_vote` - Encrypted vote ("yes" or "no", encrypted with user's pubkey using ECIES)
    ///
    /// # Payment
    /// Requires 0.002 NEAR for storage
    ///
    /// # Returns
    /// Timestamp (nanoseconds) used for vote hash calculation.
    /// Vote hash = SHA256(user + timestamp + encrypted_vote)
    ///
    /// # Notes
    /// - Vote is encrypted client-side using ECIES (secp256k1 + AES-256-GCM)
    /// - ECIES includes random nonce inside ciphertext (no separate nonce needed)
    /// - Multiple votes allowed (last real vote counts)
    /// - User can also send dummy votes for privacy (any ciphertext that doesn't decrypt to "yes"/"no")
    #[payable]
    pub fn cast_vote(
        &mut self,
        proposal_id: u64,
        encrypted_vote: String,
    ) -> u64 {
        let voter = env::predecessor_account_id();
        let attached = env::attached_deposit();

        // Only members can vote
        let member_info = self.members.get(&voter)
            .expect("Only members can vote");

        // Check if user has pubkey (completed join)
        assert!(
            self.user_pubkeys.get(&voter).is_some(),
            "Complete join first to derive encryption key"
        );

        // Check storage deposit
        assert!(
            attached.as_yoctonear() >= STORAGE_DEPOSIT_PER_VOTE,
            "Minimum deposit is {} yoctoNEAR for vote storage",
            STORAGE_DEPOSIT_PER_VOTE
        );

        // Get proposal
        let proposal = self.proposals.get(&proposal_id)
            .expect("Proposal not found");

        // Check member joined BEFORE proposal was created (prevent retroactive voting)
        // Note: joined_at = 0 means old member from migration (can vote on all proposals)
        if member_info.joined_at > 0 {
            assert!(
                member_info.joined_at < proposal.created_at,
                "Cannot vote on proposals created before you joined"
            );
        }

        // Check proposal is active
        assert!(
            proposal.status == ProposalStatus::Active,
            "Proposal is not active"
        );

        // Check deadline not passed (if deadline is set)
        if let Some(deadline_ns) = proposal.deadline {
            assert!(
                env::block_timestamp() < deadline_ns,
                "Voting deadline has passed"
            );
        }

        // Create vote with blockchain timestamp
        let timestamp = env::block_timestamp();
        let vote = Vote {
            user: voter.clone(),
            encrypted_vote,
            timestamp,
        };

        // Add vote to list
        let mut votes = self.votes.get(&proposal_id).unwrap();
        votes.push(&vote);
        self.votes.insert(&proposal_id, &votes);

        log!("Vote cast by {} on proposal {} at timestamp {}", voter, proposal_id, timestamp);

        // Return timestamp so frontend can compute vote hash immediately
        // vote_hash = SHA256(user + timestamp + encrypted_vote)
        timestamp
    }

    /// Finalize a proposal and tally votes in TEE
    ///
    /// # Arguments
    /// * `proposal_id` - Proposal ID
    ///
    /// # Payment
    /// Requires 0.01 NEAR for OutLayer execution
    ///
    /// # Process
    /// 1. User pays for OutLayer execution
    /// 2. Contract sends all encrypted votes to OutLayer
    /// 3. OutLayer WASM decrypts votes in TEE
    /// 4. OutLayer returns tally (yes_count, no_count, total)
    /// 5. Callback updates proposal status (Passed/Rejected)
    ///
    /// # Timing
    /// - Can be called **any time** after first vote is cast
    /// - Does NOT require waiting for deadline
    /// - Deadline only blocks NEW votes, not finalization
    /// - This allows early finalization if quorum is reached
    #[payable]
    pub fn finalize_proposal(&mut self, proposal_id: u64) -> Promise {
        let caller = env::predecessor_account_id();
        let attached = env::attached_deposit();

        // Check deposit for OutLayer execution
        assert!(
            attached.as_yoctonear() >= MIN_OUTLAYER_DEPOSIT,
            "Minimum deposit is {} yoctoNEAR for OutLayer execution",
            MIN_OUTLAYER_DEPOSIT
        );

        // Get proposal
        let proposal = self.proposals.get(&proposal_id)
            .expect("Proposal not found");

        // Check proposal is active
        assert!(
            proposal.status == ProposalStatus::Active,
            "Proposal is not active"
        );

        // Get all votes
        let votes = self.votes.get(&proposal_id).unwrap();
        let votes_vec: Vec<Vote> = votes.iter().collect();

        // Ensure at least one vote exists
        assert!(
            !votes_vec.is_empty(),
            "No votes to tally. Wait for at least one vote."
        );

        log!(
            "Finalizing proposal {} with {} votes. Tallying via OutLayer TEE",
            proposal_id,
            votes_vec.len()
        );

        // Call OutLayer to tally votes in TEE
        self.request_vote_tallying(proposal_id, votes_vec, attached.as_yoctonear(), caller)
    }

    // ========== Internal methods ==========

    /// Request key derivation from OutLayer
    fn request_key_derivation(&self, user: AccountId, attached_deposit: Balance) -> Promise {
        let source = serde_json::json!({
            "GitHub": {
                "repo": "https://github.com/out-layer/private-dao-example",
                "commit": "main",
                "build_target": "wasm32-wasip1"
            }
        });

        let resource_limits = serde_json::json!({
            "max_instructions": 1000000000u64,
            "max_memory_mb": 128u32,
            "max_execution_seconds": 30u64
        });

        let input_data = serde_json::json!({
            "action": "derive_pubkey",
            "dao_account": env::current_account_id(),
            "user_account": user
        });

        // Call OutLayer with secrets_ref (master secret from keymaster)
        let secrets_ref = serde_json::json!({
            "profile": "default",
            "account_id": "zavodil2.testnet"
        });

        // Call OutLayer
        ext_outlayer::ext(OUTLAYER_CONTRACT_ID.parse().unwrap())
            .with_attached_deposit(NearToken::from_yoctonear(attached_deposit))
            .with_unused_gas_weight(1)
            .request_execution(
                source,
                Some(resource_limits),
                Some(serde_json::to_string(&input_data).unwrap()),
                Some(secrets_ref),
                Some("Json".to_string()),
                Some(user.clone()), // Refund to user
            )
            .then(
                ext_self::ext(env::current_account_id())
                    .with_static_gas(CALLBACK_GAS)
                    .on_key_derived(user),
            )
    }

    /// Request vote tallying from OutLayer
    fn request_vote_tallying(
        &self,
        proposal_id: u64,
        votes: Vec<Vote>,
        attached_deposit: Balance,
        payer: AccountId,
    ) -> Promise {
        // Get proposal to pass quorum info to worker
        let proposal = self.proposals.get(&proposal_id).unwrap();

        let source = serde_json::json!({
            "GitHub": {
                "repo": "https://github.com/out-layer/private-dao-example",
                "commit": "main",
                "build_target": "wasm32-wasip1"
            }
        });

        let resource_limits = serde_json::json!({
            "max_instructions": 10000000000u64,
            "max_memory_mb": 128u32,
            "max_execution_seconds": 60u64
        });

        let input_data = serde_json::json!({
            "action": "tally_votes",
            "dao_account": env::current_account_id(),
            "proposal_id": proposal_id,
            "votes": votes,
            "quorum": proposal.quorum
        });

        // Call OutLayer with secrets_ref (master secret from keymaster)
        let secrets_ref = serde_json::json!({
            "profile": "default",
            "account_id": "zavodil2.testnet"
        });

        ext_outlayer::ext(OUTLAYER_CONTRACT_ID.parse().unwrap())
            .with_attached_deposit(NearToken::from_yoctonear(attached_deposit))
            .with_unused_gas_weight(1)
            .request_execution(
                source,
                Some(resource_limits),
                Some(serde_json::to_string(&input_data).unwrap()),
                Some(secrets_ref), // Master secret from keymaster
                Some("Json".to_string()),
                Some(payer), // Refund to payer
            )
            .then(
                ext_self::ext(env::current_account_id())
                    .with_static_gas(CALLBACK_GAS)
                    .on_votes_tallied(proposal_id),
            )
    }

    /// Callback after key derivation
    #[private]
    pub fn on_key_derived(
        &mut self,
        user: AccountId,
        #[callback_result] result: Result<Option<OutLayerResponse>, PromiseError>,
    ) {
        match result {
            Ok(Some(outlayer_response)) => {
                log!("OutLayer response for {}: success={}", user, outlayer_response.success);

                // Check if execution was successful
                if !outlayer_response.success {
                    let error_msg = outlayer_response.error.unwrap_or_else(|| "Unknown error".to_string());
                    log!("OutLayer execution failed for {}: {}", user, error_msg);
                    env::panic_str(&format!("OutLayer error: {}", error_msg));
                }

                // Parse result field to get DeriveKeyResponse
                let key_response: DeriveKeyResponse = match serde_json::from_value(outlayer_response.result) {
                    Ok(r) => r,
                    Err(e) => {
                        log!("Failed to parse key derivation result for {}: {}", user, e);
                        env::panic_str(&format!("Invalid result format: {}", e));
                    }
                };

                log!("Public key derived for {}: {}", user, key_response.pubkey);

                // Store pubkey
                self.user_pubkeys.insert(&user, &key_response.pubkey);

                // Add as member NOW (after successful key derivation)
                // This ensures user is only added if they have a valid pubkey
                if self.members.get(&user).is_none() {
                    self.members.insert(&user, &MemberInfo {
                        joined_at: env::block_timestamp(),
                    });
                    self.member_count += 1;
                    log!("User {} added to DAO with encryption key at {}", user, env::block_timestamp());
                } else {
                    log!("User {} pubkey updated (was pre-approved in private DAO)", user);
                }

                log!("User {} can now vote with encrypted ballots", user);
            }
            Ok(None) => {
                log!("OutLayer execution failed for user {}", user);
                env::panic_str("Failed to derive encryption key");
            }
            Err(e) => {
                log!("Promise error for user {}: {:?}", user, e);
                env::panic_str(&format!("Promise error: {:?}", e));
            }
        }
    }

    /// Callback after vote tallying
    #[private]
    pub fn on_votes_tallied(
        &mut self,
        proposal_id: u64,
        #[callback_result] result: Result<Option<OutLayerResponse>, PromiseError>,
    ) {
        match result {
            Ok(Some(outlayer_response)) => {
                log!("OutLayer response for proposal {}: success={}", proposal_id, outlayer_response.success);

                // Check if execution was successful
                if !outlayer_response.success {
                    let error_msg = outlayer_response.error.unwrap_or_else(|| "Unknown error".to_string());
                    log!("OutLayer execution failed for proposal {}: {}", proposal_id, error_msg);
                    env::panic_str(&format!("OutLayer error: {}", error_msg));
                }

                // Parse result field to get TallyResponse
                let response: TallyResponse = match serde_json::from_value(outlayer_response.result) {
                    Ok(r) => r,
                    Err(e) => {
                        log!("Failed to parse tally result for proposal {}: {}", proposal_id, e);
                        env::panic_str(&format!("Invalid result format: {}", e));
                    }
                };

                // Get proposal
                let mut proposal = self.proposals.get(&proposal_id).unwrap();

                // Check if vote counts are present (quorum met in TEE)
                let quorum_met = response.yes_count.is_some();

                if quorum_met {
                    let yes_count = response.yes_count.unwrap();
                    let no_count = response.no_count.unwrap();

                    log!(
                        "Votes tallied for proposal {}: YES={}, NO={}, TOTAL={}, QUORUM MET",
                        proposal_id,
                        yes_count,
                        no_count,
                        response.total_votes
                    );

                    // Determine if passed (quorum met AND more yes than no)
                    let passed = yes_count > no_count;

                    proposal.status = if passed {
                        ProposalStatus::Passed
                    } else {
                        ProposalStatus::Rejected
                    };

                    // Store full results
                    proposal.tally_result = Some(TallyResult {
                        quorum_met: true,
                        yes_count: Some(yes_count),
                        no_count: Some(no_count),
                        total_votes: response.total_votes,
                        tee_attestation: response.tee_attestation,
                        votes_merkle_root: response.votes_merkle_root.clone(),
                        merkle_proofs: response.merkle_proofs.clone(),
                    });
                } else {
                    log!(
                        "Votes tallied for proposal {}: TOTAL={}, QUORUM NOT MET (counts hidden)",
                        proposal_id,
                        response.total_votes
                    );

                    // Quorum not met - check if deadline passed
                    let deadline_passed = if let Some(deadline_ns) = proposal.deadline {
                        env::block_timestamp() >= deadline_ns
                    } else {
                        false // No deadline = never passed
                    };

                    if deadline_passed {
                        // Deadline passed + no quorum = Rejected
                        proposal.status = ProposalStatus::Rejected;
                        log!("Proposal {} rejected: deadline passed without reaching quorum", proposal_id);
                    } else {
                        // Deadline not passed or no deadline - keep Active to allow more votes
                        log!("Proposal {} remains active: quorum not met but deadline not passed", proposal_id);
                    }

                    proposal.tally_result = Some(TallyResult {
                        quorum_met: false,
                        yes_count: None,
                        no_count: None,
                        total_votes: response.total_votes,
                        tee_attestation: response.tee_attestation,
                        votes_merkle_root: response.votes_merkle_root.clone(),
                        merkle_proofs: response.merkle_proofs.clone(),
                    });
                }

                self.proposals.insert(&proposal_id, &proposal);

                log!(
                    "Proposal {} finalized: {}",
                    proposal_id,
                    match proposal.status {
                        ProposalStatus::Passed => "PASSED",
                        ProposalStatus::Rejected => "REJECTED",
                        _ => "UNKNOWN"
                    }
                );
            }
            Ok(None) => {
                log!("OutLayer execution failed for proposal {}", proposal_id);
                env::panic_str("Failed to tally votes");
            }
            Err(e) => {
                log!("Promise error for proposal {}: {:?}", proposal_id, e);
                env::panic_str(&format!("Promise error: {:?}", e));
            }
        }
    }

    /// Assert caller is owner
    fn assert_owner(&self) {
        assert_eq!(
            env::predecessor_account_id(),
            self.owner,
            "Only owner can call this method"
        );
    }

    // ========== View methods ==========

    /// Get DAO info
    pub fn get_dao_info(&self) -> DAOInfo {
        DAOInfo {
            name: self.name.clone(),
            owner: self.owner.clone(),
            membership_mode: self.membership_mode.clone(),
            member_count: self.member_count,
        }
    }

    /// Check if account is a member
    pub fn is_member(&self, account_id: AccountId) -> bool {
        self.members.get(&account_id).is_some()
    }

    /// Get member info (joined_at timestamp)
    pub fn get_member_info(&self, account_id: AccountId) -> Option<MemberInfo> {
        self.members.get(&account_id)
    }

    /// Get user's public key
    pub fn get_user_pubkey(&self, account_id: AccountId) -> Option<String> {
        self.user_pubkeys.get(&account_id)
    }

    /// Get proposal
    pub fn get_proposal(&self, proposal_id: u64) -> Option<Proposal> {
        self.proposals.get(&proposal_id)
    }

    /// Get all proposals
    pub fn get_proposals(&self, from_index: u64, limit: u64) -> Vec<Proposal> {
        self.proposals
            .values()
            .skip(from_index as usize)
            .take(limit as usize)
            .collect()
    }

    /// Get votes for a proposal (encrypted)
    pub fn get_votes(&self, proposal_id: u64) -> Vec<Vote> {
        self.votes
            .get(&proposal_id)
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }

    /// Get vote count for a proposal
    pub fn get_vote_count(&self, proposal_id: u64) -> u64 {
        self.votes
            .get(&proposal_id)
            .map(|v| v.len())
            .unwrap_or(0)
    }

    /// Get merkle proofs for user's votes in a proposal
    ///
    /// Returns proofs for all votes cast by the specified account in the proposal.
    /// Use this to verify that votes were included in the tally.
    ///
    /// Returns empty Vec if proposal not finalized or user has no votes.
    pub fn get_vote_proofs(&self, proposal_id: u64, account_id: AccountId) -> Vec<MerkleProof> {
        // Get proposal
        let proposal = match self.proposals.get(&proposal_id) {
            Some(p) => p,
            None => return Vec::new(),
        };

        // Check if finalized
        let tally_result = match proposal.tally_result {
            Some(r) => r,
            None => return Vec::new(),
        };

        // Filter proofs for this user
        tally_result.merkle_proofs
            .into_iter()
            .filter(|proof| proof.voter == account_id.as_str())
            .collect()
    }
}
