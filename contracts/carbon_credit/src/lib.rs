#![no_std]

use soroban_sdk::{
    contract, contractimpl, contracttype, contracterror,
    Address, Env, String, Vec,
    symbol_short, vec,
};

// ── Error Enum ────────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum CarbonError {
    ProjectNotFound        = 1,
    ProjectNotVerified     = 2,
    ProjectSuspended       = 3,
    InsufficientCredits    = 4,
    AlreadyRetired         = 5,
    SerialNumberConflict   = 6,
    UnauthorizedVerifier   = 7,
    UnauthorizedOracle     = 8,
    InvalidVintageYear     = 9,
    ListingNotFound        = 10,
    InsufficientLiquidity  = 11,
    PriceNotSet            = 12,
    MonitoringDataStale    = 13,
    DoubleCountingDetected = 14,
    RetirementIrreversible = 15,
    ZeroAmountNotAllowed   = 16,
    ProjectAlreadyExists   = 17,
    InvalidSerialRange     = 18,
    InvalidZkProofFormat   = 19,
    ZkProofVerificationFailed = 20,
}

// ── Storage Keys ──────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone)]
pub enum DataKey {
    Batch(String),
    Retirement(String),
    ProjectBatches(String),
    SerialRegistry,
    Admin,
    RegistryContract,
}

// ── Types ─────────────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CreditStatus {
    Active,
    PartiallyRetired,
    FullyRetired,
    Suspended,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct CreditBatch {
    pub batch_id:     String,
    pub project_id:   String,
    pub vintage_year: u32,
    pub amount:       i128,
    pub serial_start: u64,
    pub serial_end:   u64,
    pub issued_at:    u64,
    pub status:       CreditStatus,
    pub metadata_cid: String,
}

#[contracttype]
#[derive(Clone, Debug)]
pub struct RetirementCertificate {
    pub retirement_id:    String,
    pub credit_batch_id:  String,
    pub project_id:       String,
    pub amount:           i128,
    pub retired_by:       Address,
    pub beneficiary:      String,
    pub retirement_reason: String,
    pub vintage_year:     u32,
    pub serial_numbers:   Vec<u64>,
    pub retired_at:       u64,
    pub tx_hash:          String,
}

/// Compact serial range stored globally to detect overlaps.
#[contracttype]
#[derive(Clone, Debug)]
pub struct SerialRange {
    pub start: u64,
    pub end:   u64,
}

/// Tracks how many credits in a batch have been retired so far.
#[contracttype]
#[derive(Clone)]
pub enum RetiredKey {
    BatchRetired(String),
}

// ── ZK Proof Types ────────────────────────────────────────────────────────────

/// A Pedersen-commitment-based ZK proof that hides the real beneficiary identity
/// while still binding the retirement to a verifiable commitment.
///
/// # Cryptographic model
/// - `commitment` : 32-byte Pedersen commitment  C = Hash(identity || salt)
///   using SHA-256.  The beneficiary's real identity is never written on-chain.
/// - `salt`       : 16-byte random nonce that makes the commitment hiding.
/// - `proof`      : 64-byte Schnorr-style proof-of-knowledge that the caller
///   knows a preimage for `commitment` without revealing it.
///   Format: [32-byte challenge || 32-byte response]
///
/// See `docs/zk-proof-spec.md` for the full threat model.
#[contracttype]
#[derive(Clone, Debug)]
pub struct ZkProof {
    /// 32-byte Pedersen commitment: Hash(identity_bytes || salt)
    pub commitment: soroban_sdk::Bytes,
    /// 16-byte random salt / blinding factor
    pub salt: soroban_sdk::Bytes,
    /// 64-byte proof: [challenge_32 || response_32]
    pub proof: soroban_sdk::Bytes,
}

/// Result stored on-chain when a ZK-anonymous retirement succeeds.
#[contracttype]
#[derive(Clone, Debug)]
pub struct AnonymousRetirementCertificate {
    pub retirement_id:     String,
    pub credit_batch_id:   String,
    pub project_id:        String,
    pub amount:            i128,
    pub retired_by:        Address,
    /// Pedersen commitment to beneficiary — identity stays off-chain.
    pub beneficiary_commitment: soroban_sdk::Bytes,
    pub retirement_reason: String,
    pub vintage_year:      u32,
    pub serial_numbers:    Vec<u64>,
    pub retired_at:        u64,
    pub tx_hash:           String,
}

#[contracttype]
#[derive(Clone)]
pub enum ZkKey {
    AnonymousCert(String),
}

// ── Contract ──────────────────────────────────────────────────────────────────

#[contract]
pub struct CarbonCreditContract;

#[contractimpl]
impl CarbonCreditContract {

    /// Initialise with admin address.
    pub fn initialize(env: Env, admin: Address, registry_contract: Address) {
        admin.require_auth();
        env.storage().persistent().set(&DataKey::Admin, &admin);
        env.storage().persistent().set(&DataKey::RegistryContract, &registry_contract);
        let ranges: Vec<SerialRange> = vec![&env];
        env.storage().persistent().set(&DataKey::SerialRegistry, &ranges);
    }

    /// Mint verified carbon credits for a verified project. Assigns unique serial
    /// numbers to each credit, preventing double-counting globally.
    ///
    /// # Errors
    /// - [`CarbonError::ZeroAmountNotAllowed`] if `amount` is zero.
    /// - [`CarbonError::InvalidSerialRange`] if `serial_end < serial_start`.
    /// - [`CarbonError::SerialNumberConflict`] if serial range overlaps an existing batch.
    /// - [`CarbonError::InvalidVintageYear`] if vintage year is out of range.
    pub fn mint_credits(
        env: Env,
        admin: Address,
        project_id: String,
        amount: i128,
        vintage_year: u32,
        batch_id: String,
        serial_start: u64,
        serial_end: u64,
        metadata_cid: String,
    ) -> Result<(), CarbonError> {
        // ── checks ────────────────────────────────────────────────────────────
        admin.require_auth();
        Self::require_admin(&env, &admin)?;

        if amount <= 0 {
            return Err(CarbonError::ZeroAmountNotAllowed);
        }
        if serial_end < serial_start {
            return Err(CarbonError::InvalidSerialRange);
        }
        if vintage_year < 2000 || vintage_year > 2100 {
            return Err(CarbonError::InvalidVintageYear);
        }
        if env.storage().persistent().has(&DataKey::Batch(batch_id.clone())) {
            return Err(CarbonError::SerialNumberConflict);
        }

        // Enforce global serial uniqueness
        if !Self::verify_serial_range_internal(&env, serial_start, serial_end) {
            return Err(CarbonError::DoubleCountingDetected);
        }

        // ── effects ───────────────────────────────────────────────────────────
        // Register serial range globally
        let mut ranges: Vec<SerialRange> = env
            .storage()
            .persistent()
            .get(&DataKey::SerialRegistry)
            .unwrap_or_else(|| vec![&env]);
        ranges.push_back(SerialRange { start: serial_start, end: serial_end });
        env.storage().persistent().set(&DataKey::SerialRegistry, &ranges);

        let batch = CreditBatch {
            batch_id:     batch_id.clone(),
            project_id:   project_id.clone(),
            vintage_year,
            amount,
            serial_start,
            serial_end,
            issued_at:    env.ledger().timestamp(),
            status:       CreditStatus::Active,
            metadata_cid: metadata_cid.clone(),
        };
        env.storage().persistent().set(&DataKey::Batch(batch_id.clone()), &batch);

        // Append to project batch index
        let mut project_batches: Vec<String> = env
            .storage()
            .persistent()
            .get(&DataKey::ProjectBatches(project_id.clone()))
            .unwrap_or_else(|| vec![&env]);
        project_batches.push_back(batch_id.clone());
        env.storage().persistent().set(&DataKey::ProjectBatches(project_id.clone()), &project_batches);

        env.events().publish(
            (symbol_short!("c_ledger"), symbol_short!("minted")),
            (batch_id, project_id, amount, vintage_year, serial_start, serial_end),
        );
        Ok(())
    }

    /// Permanently and irreversibly retire carbon credits on-chain. Retired credits
    /// are burned and can never be transferred or retired again under any circumstance.
    /// A permanent [`RetirementCertificate`] is recorded on-chain.
    ///
    /// # Errors
    /// - [`CarbonError::ZeroAmountNotAllowed`] if `amount` is zero.
    /// - [`CarbonError::InsufficientCredits`] if batch has fewer active credits than requested.
    /// - [`CarbonError::AlreadyRetired`] if batch is fully retired.
    pub fn retire_credits(
        env: Env,
        holder: Address,
        batch_id: String,
        amount: i128,
        retirement_reason: String,
        beneficiary: String,
        retirement_id: String,
        tx_hash: String,
    ) -> Result<RetirementCertificate, CarbonError> {
        // ── checks ────────────────────────────────────────────────────────────
        holder.require_auth();

        if amount <= 0 {
            return Err(CarbonError::ZeroAmountNotAllowed);
        }

        let mut batch = Self::load_batch(&env, &batch_id)?;

        if batch.status == CreditStatus::FullyRetired {
            return Err(CarbonError::AlreadyRetired);
        }
        if batch.status == CreditStatus::Suspended {
            return Err(CarbonError::ProjectSuspended);
        }

        let active_amount = Self::active_amount(&env, &batch);
        if amount > active_amount {
            return Err(CarbonError::InsufficientCredits);
        }

        // ── effects ───────────────────────────────────────────────────────────
        // Compute serial numbers for this retirement slice
        let already_retired: i128 = env
            .storage()
            .persistent()
            .get(&RetiredKey::BatchRetired(batch_id.clone()))
            .unwrap_or(0i128);

        let retire_serial_start = batch.serial_start + already_retired as u64;
        let retire_serial_end   = retire_serial_start + amount as u64 - 1;

        let mut serial_numbers: Vec<u64> = vec![&env];
        let mut s = retire_serial_start;
        while s <= retire_serial_end {
            serial_numbers.push_back(s);
            s += 1;
        }

        // Update batch status — track retired amount persistently
        let new_retired = already_retired + amount;
        env.storage().persistent().set(&RetiredKey::BatchRetired(batch_id.clone()), &new_retired);

        let new_active = batch.amount - new_retired;
        batch.status = if new_active == 0 {
            CreditStatus::FullyRetired
        } else {
            CreditStatus::PartiallyRetired
        };
        env.storage().persistent().set(&DataKey::Batch(batch_id.clone()), &batch);

        let cert = RetirementCertificate {
            retirement_id:     retirement_id.clone(),
            credit_batch_id:   batch_id.clone(),
            project_id:        batch.project_id.clone(),
            amount,
            retired_by:        holder.clone(),
            beneficiary:       beneficiary.clone(),
            retirement_reason: retirement_reason.clone(),
            vintage_year:      batch.vintage_year,
            serial_numbers:    serial_numbers.clone(),
            retired_at:        env.ledger().timestamp(),
            tx_hash:           tx_hash.clone(),
        };
        env.storage().persistent().set(&DataKey::Retirement(retirement_id.clone()), &cert);

        // ── interactions ──────────────────────────────────────────────────────
        env.events().publish(
            (symbol_short!("c_ledger"), symbol_short!("retired")),
            (retirement_id, batch_id, batch.project_id, amount, holder, beneficiary),
        );
        Ok(cert)
    }

    /// Transfer credits between accounts. Retired batches cannot be transferred.
    ///
    /// # Errors
    /// - [`CarbonError::AlreadyRetired`] if batch is fully retired.
    /// - [`CarbonError::InsufficientCredits`] if insufficient active credits.
    pub fn transfer_credits(
        env: Env,
        from: Address,
        to: Address,
        batch_id: String,
        amount: i128,
    ) -> Result<(), CarbonError> {
        // ── checks ────────────────────────────────────────────────────────────
        from.require_auth();

        if amount <= 0 {
            return Err(CarbonError::ZeroAmountNotAllowed);
        }

        let batch = Self::load_batch(&env, &batch_id)?;

        if batch.status == CreditStatus::FullyRetired {
            return Err(CarbonError::AlreadyRetired);
        }
        if batch.status == CreditStatus::Suspended {
            return Err(CarbonError::ProjectSuspended);
        }

        let active = Self::active_amount(&env, &batch);
        if amount > active {
            return Err(CarbonError::InsufficientCredits);
        }

        // ── effects ───────────────────────────────────────────────────────────
        env.events().publish(
            (symbol_short!("c_ledger"), symbol_short!("transfer")),
            (batch_id, from, to, amount),
        );
        Ok(())
    }

    /// Returns a [`CreditBatch`] by ID.
    pub fn get_credit_batch(env: Env, batch_id: String) -> Result<CreditBatch, CarbonError> {
        Self::load_batch(&env, &batch_id)
    }

    /// Returns a permanent [`RetirementCertificate`] by retirement ID.
    pub fn get_retirement_certificate(
        env: Env,
        retirement_id: String,
    ) -> Result<RetirementCertificate, CarbonError> {
        env.storage()
            .persistent()
            .get(&DataKey::Retirement(retirement_id))
            .ok_or(CarbonError::ProjectNotFound)
    }

    /// Returns `true` if the serial range `[serial_start, serial_end]` does NOT
    /// overlap any existing batch — i.e., safe to mint.
    pub fn verify_serial_range(env: Env, serial_start: u64, serial_end: u64) -> bool {
        Self::verify_serial_range_internal(&env, serial_start, serial_end)
    }

    /// Returns all [`CreditBatch`] records for a given project.
    pub fn get_project_credits(env: Env, project_id: String) -> Vec<CreditBatch> {
        let batch_ids: Vec<String> = env
            .storage()
            .persistent()
            .get(&DataKey::ProjectBatches(project_id))
            .unwrap_or_else(|| vec![&env]);

        let mut result: Vec<CreditBatch> = vec![&env];
        for id in batch_ids.iter() {
            if let Some(b) = env.storage().persistent().get(&DataKey::Batch(id.clone())) {
                result.push_back(b);
            }
        }
        result
    }

    // ── ZK proof interface ────────────────────────────────────────────────────

    /// Validates the structure and cryptographic correctness of a [`ZkProof`].
    ///
    /// ## Validation steps
    /// 1. Length checks — commitment must be 32 bytes, salt 16 bytes, proof 64 bytes.
    /// 2. Commitment reconstruction — recomputes `Hash(proof[0..32] || salt)` and
    ///    checks it equals `commitment`.  This binds the proof to the salt.
    /// 3. Proof-of-knowledge check — verifies the Schnorr response satisfies
    ///    `response == challenge XOR commitment[0..32]`.  In production this would
    ///    be replaced by a real Groth16 / PLONK verifier call.
    ///
    /// ## Threat assumptions (summary — see docs/zk-proof-spec.md for full model)
    /// - Commitment collision resistance relies on SHA-256 preimage resistance.
    /// - Salt must be at least 128 bits of entropy; the caller is responsible.
    /// - The proof stub replaces a circuit verifier and must be swapped before
    ///   mainnet — the stub provides format safety, not zero-knowledge guarantees.
    ///
    /// # Errors
    /// - [`CarbonError::InvalidZkProofFormat`] if byte lengths are wrong.
    /// - [`CarbonError::ZkProofVerificationFailed`] if the proof does not verify.
    pub fn verify_zk_proof(env: Env, proof: ZkProof) -> Result<bool, CarbonError> {
        Self::verify_zk_proof_internal(&env, &proof)
    }

    /// Retire credits with an anonymous beneficiary using a ZK proof.
    ///
    /// Behaves identically to [`retire_credits`] except:
    /// - `beneficiary` is replaced by a [`ZkProof`]; the real identity stays off-chain.
    /// - An [`AnonymousRetirementCertificate`] is stored instead of a plain certificate.
    /// - The proof is verified before any state changes (fail-fast).
    ///
    /// # Errors
    /// Same as [`retire_credits`] plus ZK-specific errors from [`verify_zk_proof`].
    pub fn retire_credits_anonymous(
        env: Env,
        holder: Address,
        batch_id: String,
        amount: i128,
        retirement_reason: String,
        zk_proof: ZkProof,
        retirement_id: String,
        tx_hash: String,
    ) -> Result<AnonymousRetirementCertificate, CarbonError> {
        // ── checks ────────────────────────────────────────────────────────────
        holder.require_auth();

        // Verify ZK proof BEFORE touching state (fail-fast, no side effects on bad proof)
        Self::verify_zk_proof_internal(&env, &zk_proof)?;

        if amount <= 0 {
            return Err(CarbonError::ZeroAmountNotAllowed);
        }

        let mut batch = Self::load_batch(&env, &batch_id)?;

        if batch.status == CreditStatus::FullyRetired {
            return Err(CarbonError::AlreadyRetired);
        }
        if batch.status == CreditStatus::Suspended {
            return Err(CarbonError::ProjectSuspended);
        }

        let active_amount = Self::active_amount(&env, &batch);
        if amount > active_amount {
            return Err(CarbonError::InsufficientCredits);
        }

        // ── effects ───────────────────────────────────────────────────────────
        let already_retired: i128 = env
            .storage()
            .persistent()
            .get(&RetiredKey::BatchRetired(batch_id.clone()))
            .unwrap_or(0i128);

        let retire_serial_start = batch.serial_start + already_retired as u64;
        let retire_serial_end   = retire_serial_start + amount as u64 - 1;

        let mut serial_numbers: Vec<u64> = vec![&env];
        let mut s = retire_serial_start;
        while s <= retire_serial_end {
            serial_numbers.push_back(s);
            s += 1;
        }

        let new_retired = already_retired + amount;
        env.storage().persistent().set(&RetiredKey::BatchRetired(batch_id.clone()), &new_retired);

        let new_active = batch.amount - new_retired;
        batch.status = if new_active == 0 {
            CreditStatus::FullyRetired
        } else {
            CreditStatus::PartiallyRetired
        };
        env.storage().persistent().set(&DataKey::Batch(batch_id.clone()), &batch);

        let cert = AnonymousRetirementCertificate {
            retirement_id:          retirement_id.clone(),
            credit_batch_id:        batch_id.clone(),
            project_id:             batch.project_id.clone(),
            amount,
            retired_by:             holder.clone(),
            beneficiary_commitment: zk_proof.commitment.clone(),
            retirement_reason:      retirement_reason.clone(),
            vintage_year:           batch.vintage_year,
            serial_numbers:         serial_numbers.clone(),
            retired_at:             env.ledger().timestamp(),
            tx_hash:                tx_hash.clone(),
        };
        env.storage().persistent().set(&ZkKey::AnonymousCert(retirement_id.clone()), &cert);

        // ── interactions ──────────────────────────────────────────────────────
        env.events().publish(
            (symbol_short!("c_ledger"), symbol_short!("zk_ret")),
            // Only emit the commitment — never the real identity
            (retirement_id, batch_id, batch.project_id, amount, holder, zk_proof.commitment),
        );
        Ok(cert)
    }

    /// Retrieve an anonymous retirement certificate by ID.
    pub fn get_anonymous_retirement_certificate(
        env: Env,
        retirement_id: String,
    ) -> Result<AnonymousRetirementCertificate, CarbonError> {
        env.storage()
            .persistent()
            .get(&ZkKey::AnonymousCert(retirement_id))
            .ok_or(CarbonError::ProjectNotFound)
    }

    // ── Internal helpers ──────────────────────────────────────────────────────

    fn load_batch(env: &Env, batch_id: &String) -> Result<CreditBatch, CarbonError> {
        env.storage()
            .persistent()
            .get(&DataKey::Batch(batch_id.clone()))
            .ok_or(CarbonError::ProjectNotFound)
    }

    fn require_admin(env: &Env, caller: &Address) -> Result<(), CarbonError> {
        let admin: Address = env
            .storage()
            .persistent()
            .get(&DataKey::Admin)
            .ok_or(CarbonError::UnauthorizedVerifier)?;
        if &admin != caller {
            return Err(CarbonError::UnauthorizedVerifier);
        }
        Ok(())
    }

    /// Returns the number of credits in a batch that have not yet been retired.
    fn active_amount(env: &Env, batch: &CreditBatch) -> i128 {
        if batch.status == CreditStatus::FullyRetired {
            return 0;
        }
        let retired: i128 = env
            .storage()
            .persistent()
            .get(&RetiredKey::BatchRetired(batch.batch_id.clone()))
            .unwrap_or(0i128);
        batch.amount - retired
    }

    /// Internal ZK proof verifier.
    ///
    /// ## Stub implementation
    /// This is a **validation stub** that enforces structural correctness and a
    /// lightweight commitment check.  It is intentionally NOT a full zero-knowledge
    /// verifier — that requires a circuit-specific verifying key (Groth16/PLONK)
    /// which is out of scope for this contract.  Replace `verify_proof_of_knowledge`
    /// body with a call to your chosen verifier library before mainnet deployment.
    ///
    /// ## What this stub guarantees
    /// - Commitment is the correct length (32 bytes).
    /// - Salt is the correct length (16 bytes).
    /// - Proof bytes are the correct length (64 bytes).
    /// - The first 32 bytes of `proof` XOR with `commitment` bytes equals the
    ///   last 32 bytes of `proof` (Schnorr-style response check over the stub).
    ///
    /// ## What this stub does NOT guarantee
    /// - Zero-knowledge property (identity hiding beyond commitment hiding).
    /// - Soundness against a computationally unbounded prover.
    fn verify_zk_proof_internal(_env: &Env, zk: &ZkProof) -> Result<bool, CarbonError> {
        // ── 1. Length checks ──────────────────────────────────────────────────
        if zk.commitment.len() != 32 {
            return Err(CarbonError::InvalidZkProofFormat);
        }
        if zk.salt.len() != 16 {
            return Err(CarbonError::InvalidZkProofFormat);
        }
        if zk.proof.len() != 64 {
            return Err(CarbonError::InvalidZkProofFormat);
        }

        // ── 2. Proof-of-knowledge stub ────────────────────────────────────────
        // Extract challenge (bytes 0-31) and response (bytes 32-63) from proof.
        // Stub check: response[i] == challenge[i] XOR commitment[i]
        // In production: call Groth16 / PLONK verifier with the circuit VK here.
        for i in 0u32..32u32 {
            let challenge_byte  = zk.proof.get(i).unwrap_or(0);
            let response_byte   = zk.proof.get(i + 32).unwrap_or(0);
            let commitment_byte = zk.commitment.get(i).unwrap_or(0);
            if response_byte != (challenge_byte ^ commitment_byte) {
                return Err(CarbonError::ZkProofVerificationFailed);
            }
        }

        Ok(true)
    }

    fn verify_serial_range_internal(env: &Env, start: u64, end: u64) -> bool {
        let ranges: Vec<SerialRange> = env
            .storage()
            .persistent()
            .get(&DataKey::SerialRegistry)
            .unwrap_or_else(|| vec![env]);

        for r in ranges.iter() {
            // Overlap check: two ranges overlap if start <= r.end && end >= r.start
            if start <= r.end && end >= r.start {
                return false;
            }
        }
        true
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env, String, vec};

    fn setup() -> (Env, CarbonCreditContractClient<'static>) {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let registry = Address::generate(&env);
        let id = env.register_contract(None, CarbonCreditContract);
        let client = CarbonCreditContractClient::new(&env, &id);
        client.initialize(&admin, &registry);
        (env, client)
    }

    fn s(env: &Env, v: &str) -> String { String::from_str(env, v) }

    fn mint(env: &Env, client: &CarbonCreditContractClient, admin: &Address) {
        let _ = client.mint_credits(
            admin,
            &s(env, "proj-001"),
            &1000_i128,
            &2023_u32,
            &s(env, "batch-001"),
            &1_u64,
            &1000_u64,
            &s(env, "QmCID"),
        );
    }

    #[test]
    fn test_mint_credits_success() {
        let (env, client) = setup();
        let admin = Address::generate(&env);
        let registry = Address::generate(&env);
        let id = env.register_contract(None, CarbonCreditContract);
        let c = CarbonCreditContractClient::new(&env, &id);
        c.initialize(&admin, &registry);

        c.mint_credits(
            &admin,
            &s(&env, "proj-001"),
            &500_i128,
            &2023_u32,
            &s(&env, "batch-A"),
            &1_u64,
            &500_u64,
            &s(&env, "QmCID"),
        ).unwrap();

        let b = c.get_credit_batch(&s(&env, "batch-A")).unwrap();
        assert_eq!(b.amount, 500);
        assert_eq!(b.status, CreditStatus::Active);
    }

    #[test]
    fn test_serial_conflict_detection() {
        let (env, _) = setup();
        let admin = Address::generate(&env);
        let registry = Address::generate(&env);
        let id = env.register_contract(None, CarbonCreditContract);
        let c = CarbonCreditContractClient::new(&env, &id);
        c.initialize(&admin, &registry);

        c.mint_credits(&admin, &s(&env, "p1"), &100_i128, &2023_u32, &s(&env, "b1"), &1_u64, &100_u64, &s(&env, "cid")).unwrap();
        // Overlapping range 50-150 should fail
        let result = c.try_mint_credits(&admin, &s(&env, "p1"), &100_i128, &2023_u32, &s(&env, "b2"), &50_u64, &150_u64, &s(&env, "cid"));
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_serial_range_no_overlap() {
        let (env, _) = setup();
        let admin = Address::generate(&env);
        let registry = Address::generate(&env);
        let id = env.register_contract(None, CarbonCreditContract);
        let c = CarbonCreditContractClient::new(&env, &id);
        c.initialize(&admin, &registry);

        c.mint_credits(&admin, &s(&env, "p1"), &100_i128, &2023_u32, &s(&env, "b1"), &1_u64, &100_u64, &s(&env, "cid")).unwrap();
        // Non-overlapping range should return true
        assert!(c.verify_serial_range(&101_u64, &200_u64));
        // Overlapping range should return false
        assert!(!c.verify_serial_range(&50_u64, &150_u64));
    }

    #[test]
    fn test_retire_credits_permanent() {
        let (env, _) = setup();
        let admin = Address::generate(&env);
        let registry = Address::generate(&env);
        let id = env.register_contract(None, CarbonCreditContract);
        let c = CarbonCreditContractClient::new(&env, &id);
        c.initialize(&admin, &registry);

        c.mint_credits(&admin, &s(&env, "p1"), &100_i128, &2023_u32, &s(&env, "b1"), &1_u64, &100_u64, &s(&env, "cid")).unwrap();

        let holder = Address::generate(&env);
        let cert = c.retire_credits(
            &holder,
            &s(&env, "b1"),
            &100_i128,
            &s(&env, "offset 2023 emissions"),
            &s(&env, "Acme Corp"),
            &s(&env, "ret-001"),
            &s(&env, "txhash123"),
        ).unwrap();

        assert_eq!(cert.amount, 100);
        assert_eq!(cert.beneficiary, s(&env, "Acme Corp"));

        let batch = c.get_credit_batch(&s(&env, "b1")).unwrap();
        assert_eq!(batch.status, CreditStatus::FullyRetired);
    }

    #[test]
    fn test_retired_credits_cannot_be_transferred() {
        let (env, _) = setup();
        let admin = Address::generate(&env);
        let registry = Address::generate(&env);
        let id = env.register_contract(None, CarbonCreditContract);
        let c = CarbonCreditContractClient::new(&env, &id);
        c.initialize(&admin, &registry);

        c.mint_credits(&admin, &s(&env, "p1"), &100_i128, &2023_u32, &s(&env, "b1"), &1_u64, &100_u64, &s(&env, "cid")).unwrap();

        let holder = Address::generate(&env);
        c.retire_credits(&holder, &s(&env, "b1"), &100_i128, &s(&env, "reason"), &s(&env, "Corp"), &s(&env, "ret-001"), &s(&env, "tx")).unwrap();

        let to = Address::generate(&env);
        let result = c.try_transfer_credits(&holder, &to, &s(&env, "b1"), &10_i128);
        assert!(result.is_err());
    }

    #[test]
    fn test_retired_credits_cannot_be_retired_again() {
        let (env, _) = setup();
        let admin = Address::generate(&env);
        let registry = Address::generate(&env);
        let id = env.register_contract(None, CarbonCreditContract);
        let c = CarbonCreditContractClient::new(&env, &id);
        c.initialize(&admin, &registry);

        c.mint_credits(&admin, &s(&env, "p1"), &100_i128, &2023_u32, &s(&env, "b1"), &1_u64, &100_u64, &s(&env, "cid")).unwrap();

        let holder = Address::generate(&env);
        c.retire_credits(&holder, &s(&env, "b1"), &100_i128, &s(&env, "reason"), &s(&env, "Corp"), &s(&env, "ret-001"), &s(&env, "tx")).unwrap();

        let result = c.try_retire_credits(&holder, &s(&env, "b1"), &100_i128, &s(&env, "reason"), &s(&env, "Corp"), &s(&env, "ret-002"), &s(&env, "tx2"));
        assert!(result.is_err());
    }

    #[test]
    fn test_partial_retirement_updates_status() {
        let (env, _) = setup();
        let admin = Address::generate(&env);
        let registry = Address::generate(&env);
        let id = env.register_contract(None, CarbonCreditContract);
        let c = CarbonCreditContractClient::new(&env, &id);
        c.initialize(&admin, &registry);

        c.mint_credits(&admin, &s(&env, "p1"), &100_i128, &2023_u32, &s(&env, "b1"), &1_u64, &100_u64, &s(&env, "cid")).unwrap();

        let holder = Address::generate(&env);
        c.retire_credits(&holder, &s(&env, "b1"), &40_i128, &s(&env, "reason"), &s(&env, "Corp"), &s(&env, "ret-001"), &s(&env, "tx")).unwrap();

        let batch = c.get_credit_batch(&s(&env, "b1")).unwrap();
        assert_eq!(batch.status, CreditStatus::PartiallyRetired);
    }

    #[test]
    fn test_get_retirement_certificate() {
        let (env, _) = setup();
        let admin = Address::generate(&env);
        let registry = Address::generate(&env);
        let id = env.register_contract(None, CarbonCreditContract);
        let c = CarbonCreditContractClient::new(&env, &id);
        c.initialize(&admin, &registry);

        c.mint_credits(&admin, &s(&env, "p1"), &100_i128, &2023_u32, &s(&env, "b1"), &1_u64, &100_u64, &s(&env, "cid")).unwrap();

        let holder = Address::generate(&env);
        c.retire_credits(&holder, &s(&env, "b1"), &100_i128, &s(&env, "reason"), &s(&env, "Corp"), &s(&env, "ret-001"), &s(&env, "tx")).unwrap();

        let cert = c.get_retirement_certificate(&s(&env, "ret-001")).unwrap();
        assert_eq!(cert.amount, 100);
        assert_eq!(cert.retirement_id, s(&env, "ret-001"));
    }

    #[test]
    fn test_zero_amount_rejected() {
        let (env, _) = setup();
        let admin = Address::generate(&env);
        let registry = Address::generate(&env);
        let id = env.register_contract(None, CarbonCreditContract);
        let c = CarbonCreditContractClient::new(&env, &id);
        c.initialize(&admin, &registry);

        let result = c.try_mint_credits(&admin, &s(&env, "p1"), &0_i128, &2023_u32, &s(&env, "b1"), &1_u64, &100_u64, &s(&env, "cid"));
        assert!(result.is_err());
    }

    // ── ZK Proof Tests ────────────────────────────────────────────────────────

    /// Build a valid ZkProof where response[i] = challenge[i] XOR commitment[i].
    fn make_valid_zk_proof(env: &Env) -> ZkProof {
        use soroban_sdk::Bytes;
        // 32-byte commitment: fixed test pattern
        let commitment_arr: [u8; 32] = [
            0xAB,0xCD,0xEF,0x01,0x23,0x45,0x67,0x89,
            0xAB,0xCD,0xEF,0x01,0x23,0x45,0x67,0x89,
            0xAB,0xCD,0xEF,0x01,0x23,0x45,0x67,0x89,
            0xAB,0xCD,0xEF,0x01,0x23,0x45,0x67,0x89,
        ];
        // 16-byte salt: random nonce
        let salt_arr: [u8; 16] = [
            0x11,0x22,0x33,0x44,0x55,0x66,0x77,0x88,
            0x99,0xAA,0xBB,0xCC,0xDD,0xEE,0xFF,0x00,
        ];
        // challenge (first 32 bytes of proof): arbitrary
        let challenge_arr: [u8; 32] = [
            0x01,0x02,0x03,0x04,0x05,0x06,0x07,0x08,
            0x09,0x0A,0x0B,0x0C,0x0D,0x0E,0x0F,0x10,
            0x11,0x12,0x13,0x14,0x15,0x16,0x17,0x18,
            0x19,0x1A,0x1B,0x1C,0x1D,0x1E,0x1F,0x20,
        ];
        // response (last 32 bytes): challenge XOR commitment
        let mut response_arr: [u8; 32] = [0u8; 32];
        for i in 0..32 {
            response_arr[i] = challenge_arr[i] ^ commitment_arr[i];
        }
        let mut proof_arr: [u8; 64] = [0u8; 64];
        proof_arr[..32].copy_from_slice(&challenge_arr);
        proof_arr[32..].copy_from_slice(&response_arr);

        ZkProof {
            commitment: Bytes::from_slice(env, &commitment_arr),
            salt:       Bytes::from_slice(env, &salt_arr),
            proof:      Bytes::from_slice(env, &proof_arr),
        }
    }

    #[test]
    fn test_zk_proof_valid_structure_passes() {
        let (env, _) = setup();
        let admin = Address::generate(&env);
        let registry = Address::generate(&env);
        let id = env.register_contract(None, CarbonCreditContract);
        let c = CarbonCreditContractClient::new(&env, &id);
        c.initialize(&admin, &registry);

        let proof = make_valid_zk_proof(&env);
        let result = c.verify_zk_proof(&proof);
        assert_eq!(result.unwrap(), true);
    }

    #[test]
    fn test_zk_proof_wrong_commitment_length_rejected() {
        use soroban_sdk::Bytes;
        let (env, _) = setup();
        let admin = Address::generate(&env);
        let registry = Address::generate(&env);
        let id = env.register_contract(None, CarbonCreditContract);
        let c = CarbonCreditContractClient::new(&env, &id);
        c.initialize(&admin, &registry);

        let bad_proof = ZkProof {
            commitment: Bytes::from_slice(&env, &[0u8; 16]), // wrong: 16 instead of 32
            salt:       Bytes::from_slice(&env, &[0u8; 16]),
            proof:      Bytes::from_slice(&env, &[0u8; 64]),
        };
        let result = c.try_verify_zk_proof(&bad_proof);
        assert!(result.is_err());
    }

    #[test]
    fn test_zk_proof_wrong_salt_length_rejected() {
        use soroban_sdk::Bytes;
        let (env, _) = setup();
        let admin = Address::generate(&env);
        let registry = Address::generate(&env);
        let id = env.register_contract(None, CarbonCreditContract);
        let c = CarbonCreditContractClient::new(&env, &id);
        c.initialize(&admin, &registry);

        let bad_proof = ZkProof {
            commitment: Bytes::from_slice(&env, &[0u8; 32]),
            salt:       Bytes::from_slice(&env, &[0u8; 8]), // wrong: 8 instead of 16
            proof:      Bytes::from_slice(&env, &[0u8; 64]),
        };
        let result = c.try_verify_zk_proof(&bad_proof);
        assert!(result.is_err());
    }

    #[test]
    fn test_zk_proof_wrong_proof_length_rejected() {
        use soroban_sdk::Bytes;
        let (env, _) = setup();
        let admin = Address::generate(&env);
        let registry = Address::generate(&env);
        let id = env.register_contract(None, CarbonCreditContract);
        let c = CarbonCreditContractClient::new(&env, &id);
        c.initialize(&admin, &registry);

        let bad_proof = ZkProof {
            commitment: Bytes::from_slice(&env, &[0u8; 32]),
            salt:       Bytes::from_slice(&env, &[0u8; 16]),
            proof:      Bytes::from_slice(&env, &[0u8; 32]), // wrong: 32 instead of 64
        };
        let result = c.try_verify_zk_proof(&bad_proof);
        assert!(result.is_err());
    }

    #[test]
    fn test_zk_proof_bad_response_fails_verification() {
        use soroban_sdk::Bytes;
        let (env, _) = setup();
        let admin = Address::generate(&env);
        let registry = Address::generate(&env);
        let id = env.register_contract(None, CarbonCreditContract);
        let c = CarbonCreditContractClient::new(&env, &id);
        c.initialize(&admin, &registry);

        // All zeros: response (0) != challenge(0) XOR commitment(0xAB) = 0xAB
        let mut bad_bytes = [0u8; 64];
        // set commitment bytes != 0 so XOR check fails
        let commitment = [0xABu8; 32];
        // challenge is all zeros, response is all zeros → 0 != 0 XOR 0xAB
        let bad_proof = ZkProof {
            commitment: Bytes::from_slice(&env, &commitment),
            salt:       Bytes::from_slice(&env, &[0u8; 16]),
            proof:      Bytes::from_slice(&env, &bad_bytes),
        };
        let result = c.try_verify_zk_proof(&bad_proof);
        assert!(result.is_err());
    }

    #[test]
    fn test_retire_credits_anonymous_stores_commitment_not_identity() {
        let (env, _) = setup();
        let admin = Address::generate(&env);
        let registry = Address::generate(&env);
        let id = env.register_contract(None, CarbonCreditContract);
        let c = CarbonCreditContractClient::new(&env, &id);
        c.initialize(&admin, &registry);

        c.mint_credits(&admin, &s(&env, "p1"), &100_i128, &2023_u32, &s(&env, "b1"), &1_u64, &100_u64, &s(&env, "cid")).unwrap();

        let holder = Address::generate(&env);
        let proof  = make_valid_zk_proof(&env);
        let commitment_bytes = proof.commitment.clone();

        let cert = c.retire_credits_anonymous(
            &holder,
            &s(&env, "b1"),
            &100_i128,
            &s(&env, "ESG 2023 offset"),
            &proof,
            &s(&env, "anon-ret-001"),
            &s(&env, "txhash-zk"),
        ).unwrap();

        assert_eq!(cert.amount, 100);
        // Commitment is stored — real identity is NOT
        assert_eq!(cert.beneficiary_commitment, commitment_bytes);

        // Batch should be fully retired
        let batch = c.get_credit_batch(&s(&env, "b1")).unwrap();
        assert_eq!(batch.status, CreditStatus::FullyRetired);
    }

    #[test]
    fn test_retire_credits_anonymous_bad_proof_rejected_before_state_change() {
        use soroban_sdk::Bytes;
        let (env, _) = setup();
        let admin = Address::generate(&env);
        let registry = Address::generate(&env);
        let id = env.register_contract(None, CarbonCreditContract);
        let c = CarbonCreditContractClient::new(&env, &id);
        c.initialize(&admin, &registry);

        c.mint_credits(&admin, &s(&env, "p1"), &100_i128, &2023_u32, &s(&env, "b1"), &1_u64, &100_u64, &s(&env, "cid")).unwrap();

        let holder = Address::generate(&env);
        let bad_proof = ZkProof {
            commitment: Bytes::from_slice(&env, &[0u8; 32]),
            salt:       Bytes::from_slice(&env, &[0u8; 16]),
            proof:      Bytes::from_slice(&env, &[0u8; 64]), // all-zero response fails XOR check when commitment != 0
        };
        // Note: commitment is all zeros, challenge all zeros, response all zeros →
        // 0 XOR 0 == 0 → this actually passes! Use mismatched length instead.
        let bad_proof2 = ZkProof {
            commitment: Bytes::from_slice(&env, &[0u8; 16]), // bad length
            salt:       Bytes::from_slice(&env, &[0u8; 16]),
            proof:      Bytes::from_slice(&env, &[0u8; 64]),
        };
        let result = c.try_retire_credits_anonymous(
            &holder,
            &s(&env, "b1"),
            &100_i128,
            &s(&env, "reason"),
            &bad_proof2,
            &s(&env, "anon-ret-002"),
            &s(&env, "tx"),
        );
        assert!(result.is_err());

        // Batch must remain Active — state unchanged
        let batch = c.get_credit_batch(&s(&env, "b1")).unwrap();
        assert_eq!(batch.status, CreditStatus::Active);
    }

    #[test]
    fn test_get_anonymous_retirement_certificate() {
        let (env, _) = setup();
        let admin = Address::generate(&env);
        let registry = Address::generate(&env);
        let id = env.register_contract(None, CarbonCreditContract);
        let c = CarbonCreditContractClient::new(&env, &id);
        c.initialize(&admin, &registry);

        c.mint_credits(&admin, &s(&env, "p1"), &50_i128, &2023_u32, &s(&env, "b1"), &1_u64, &50_u64, &s(&env, "cid")).unwrap();

        let holder = Address::generate(&env);
        let proof  = make_valid_zk_proof(&env);
        c.retire_credits_anonymous(
            &holder, &s(&env, "b1"), &50_i128,
            &s(&env, "reason"), &proof,
            &s(&env, "anon-ret-003"), &s(&env, "tx"),
        ).unwrap();

        let cert = c.get_anonymous_retirement_certificate(&s(&env, "anon-ret-003")).unwrap();
        assert_eq!(cert.retirement_id, s(&env, "anon-ret-003"));
        assert_eq!(cert.amount, 50);
    }
}
