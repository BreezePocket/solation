use anchor_lang::prelude::*;

/// Option strategy types
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum StrategyType {
    /// Covered call - user sells call, deposits underlying asset
    CoveredCall = 0,
    /// Cash-secured put - user sells put, deposits USDC
    CashSecuredPut = 1,
}

/// Status of an intent in the system
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum IntentStatus {
    /// Awaiting MM fill
    Pending,
    /// MM filled, Position created
    Filled,
    /// Fill deadline passed, escrow returned
    Expired,
    /// User cancelled before fill
    Cancelled,
    /// Flagged for owner review
    Disputed,
    /// Owner resolved in favor of user
    ResolvedToUser,
    /// Owner resolved in favor of MM  
    ResolvedToMM,
    /// Owner split the escrow
    ResolvedSplit,
}

/// Intent account - represents a user's intent to open a position based on an off-chain quote
#[account]
pub struct Intent {
    /// Unique intent ID
    pub intent_id: u64,
    /// User who created the intent
    pub user: Pubkey,
    /// Market maker expected to fill
    pub market_maker: Pubkey,
    
    // Quote data (from off-chain signed message)
    /// Underlying asset mint
    pub asset_mint: Pubkey,
    /// Quote currency mint (USDC)
    pub quote_mint: Pubkey,
    /// Strategy type
    pub strategy: StrategyType,
    /// Strike price in quote decimals
    pub strike_price: u64,
    /// Premium per contract from MM's quote
    pub premium_per_contract: u64,
    /// Number of contracts
    pub contract_size: u64,
    /// When the quote expires
    pub quote_expiry: i64,
    
    // Signature verification
    /// MM's Ed25519 signature over the quote
    pub quote_signature: [u8; 64],
    /// Nonce to prevent replay attacks
    pub quote_nonce: u64,
    
    // Escrow state
    /// User's escrow PDA holding locked funds
    pub user_escrow: Pubkey,
    /// Amount locked in escrow
    pub escrow_amount: u64,
    
    // Timing
    /// When intent was created
    pub created_at: i64,
    /// MM must fill by this time
    pub fill_deadline: i64,
    
    // Dispute tracking
    /// Who flagged the dispute (if any)
    pub disputed_by: Option<Pubkey>,
    /// Reason for dispute
    pub dispute_reason: Option<String>,
    
    /// Current status
    pub status: IntentStatus,
    /// PDA bump
    pub bump: u8,
}

impl Intent {
    /// Maximum length for dispute reason string
    pub const MAX_DISPUTE_REASON_LEN: usize = 200;
    
    pub const LEN: usize = 8 +   // discriminator
        8 +   // intent_id
        32 +  // user
        32 +  // market_maker
        32 +  // asset_mint
        32 +  // quote_mint
        1 +   // strategy
        8 +   // strike_price
        8 +   // premium_per_contract
        8 +   // contract_size
        8 +   // quote_expiry
        64 +  // quote_signature
        8 +   // quote_nonce
        32 +  // user_escrow
        8 +   // escrow_amount
        8 +   // created_at
        8 +   // fill_deadline
        1 + 32 +  // disputed_by (Option<Pubkey>)
        4 + Self::MAX_DISPUTE_REASON_LEN +  // dispute_reason (Option<String>)
        1 +   // status
        1;    // bump

    pub fn is_pending(&self) -> bool {
        self.status == IntentStatus::Pending
    }

    pub fn is_disputed(&self) -> bool {
        self.status == IntentStatus::Disputed
    }

    pub fn is_expired(&self, current_timestamp: i64) -> bool {
        current_timestamp > self.fill_deadline
    }

    pub fn can_be_resolved(&self) -> bool {
        matches!(self.status, IntentStatus::Pending | IntentStatus::Disputed)
    }

    pub fn calculate_total_premium(&self) -> u64 {
        self.premium_per_contract.saturating_mul(self.contract_size)
    }
}
