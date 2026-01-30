// PDA Seeds
pub const GLOBAL_STATE_SEED: &[u8] = b"global_state";
pub const MARKET_MAKER_SEED: &[u8] = b"market_maker";
pub const MM_VAULT_SEED: &[u8] = b"mm_vault";
pub const VAULT_TOKEN_ACCOUNT_SEED: &[u8] = b"vault_token_account";
pub const QUOTE_SEED: &[u8] = b"quote";
pub const POSITION_SEED: &[u8] = b"position";
pub const POSITION_USER_VAULT_SEED: &[u8] = b"position_user_vault";
pub const POSITION_MM_VAULT_SEED: &[u8] = b"position_mm_vault";
pub const ASSET_CONFIG_SEED: &[u8] = b"asset_config";
pub const POSITION_REQUEST_SEED: &[u8] = b"position_request";

// New seeds for off-chain RFQ system
pub const INTENT_SEED: &[u8] = b"intent";
pub const MM_REGISTRY_SEED: &[u8] = b"mm_registry";
pub const NONCE_TRACKER_SEED: &[u8] = b"nonce_tracker";
pub const USER_ESCROW_SEED: &[u8] = b"user_escrow";

// MM Confirmation Window (seconds)
pub const MM_CONFIRMATION_WINDOW: i64 = 30;

// Intent fill timeout (seconds) - same as confirmation window
pub const INTENT_FILL_TIMEOUT: i64 = 30;

// Pyth parameters
pub const PYTH_STALENESS_THRESHOLD: u64 = 60; // 60 seconds

// Quote parameters
pub const MAX_STRIKES_PER_QUOTE: usize = 10;

// Basis points (10000 = 100%)
pub const BASIS_POINTS_DIVISOR: u64 = 10000;

// Dispute reason max length
pub const MAX_DISPUTE_REASON_LEN: usize = 200;

