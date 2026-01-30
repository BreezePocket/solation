use anchor_lang::prelude::*;

/// Market Maker Registry - on-chain registration of MMs with their signing keys
#[account]
pub struct MMRegistry {
    /// Owner wallet of the market maker
    pub owner: Pubkey,
    /// Ed25519 public key used for signing quotes
    pub signing_key: Pubkey,
    /// Whether this MM is active and can receive intents
    pub active: bool,
    /// Total number of intents this MM has filled
    pub total_intents_filled: u64,
    /// Total number of intents that expired (MM didn't fill)
    pub total_intents_expired: u64,
    /// Total volume traded in quote currency
    pub total_volume: u64,
    /// Reputation score (higher is better, updated by owner/backend)
    pub reputation_score: u32,
    /// Last time this MM was active
    pub last_active: i64,
    /// When this MM registered
    pub registered_at: i64,
    /// PDA bump
    pub bump: u8,
}

impl MMRegistry {
    pub const LEN: usize = 8 +   // discriminator
        32 +  // owner
        32 +  // signing_key
        1 +   // active
        8 +   // total_intents_filled
        8 +   // total_intents_expired
        8 +   // total_volume
        4 +   // reputation_score
        8 +   // last_active
        8 +   // registered_at
        1;    // bump

    /// Calculate fill rate as percentage (0-100)
    pub fn fill_rate(&self) -> u8 {
        let total = self.total_intents_filled + self.total_intents_expired;
        if total == 0 {
            return 100; // New MM gets benefit of doubt
        }
        ((self.total_intents_filled as u128 * 100) / total as u128) as u8
    }

    /// Update reputation based on fill/expire
    pub fn record_fill(&mut self, volume: u64, timestamp: i64) {
        self.total_intents_filled = self.total_intents_filled.saturating_add(1);
        self.total_volume = self.total_volume.saturating_add(volume);
        self.last_active = timestamp;
        // Slight reputation boost for fills
        self.reputation_score = self.reputation_score.saturating_add(1);
    }

    pub fn record_expire(&mut self) {
        self.total_intents_expired = self.total_intents_expired.saturating_add(1);
        // Reputation penalty for expires
        self.reputation_score = self.reputation_score.saturating_sub(10);
    }
}
