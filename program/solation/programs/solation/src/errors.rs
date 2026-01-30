use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    #[msg("The protocol is currently paused")]
    ProtocolPaused,

    #[msg("This asset is not enabled for trading")]
    AssetNotEnabled,

    #[msg("Insufficient liquidity available in market maker vault")]
    InsufficientLiquidity,

    #[msg("Quote has expired")]
    QuoteExpired,

    #[msg("Quote is not active")]
    QuoteNotActive,

    #[msg("Strike price not found in quote")]
    StrikePriceNotFound,

    #[msg("Contract size below minimum")]
    ContractSizeTooSmall,

    #[msg("Contract size above maximum")]
    ContractSizeTooLarge,

    #[msg("Position has not expired yet")]
    PositionNotExpired,

    #[msg("Position is not active")]
    PositionNotActive,

    #[msg("Position has already been settled")]
    PositionAlreadySettled,

    #[msg("Pyth price is too stale")]
    PriceTooStale,

    #[msg("Pyth feed ID mismatch")]
    PythFeedIdMismatch,

    #[msg("Invalid strike price range")]
    InvalidStrikeRange,

    #[msg("Invalid expiry range")]
    InvalidExpiryRange,

    #[msg("Math overflow")]
    MathOverflow,

    #[msg("Unauthorized")]
    Unauthorized,

    #[msg("Market maker is not active")]
    MarketMakerNotActive,

    #[msg("Too many strikes in quote")]
    TooManyStrikes,

    #[msg("Invalid quote parameters")]
    InvalidQuoteParameters,

    #[msg("Position request has expired")]
    RequestExpired,

    #[msg("Position request is not in pending status")]
    RequestNotPending,

    #[msg("Position request has not expired yet")]
    RequestNotExpired,

    #[msg("Only the market maker can confirm this request")]
    UnauthorizedConfirmation,

    // ===== New errors for off-chain RFQ system =====

    #[msg("Market maker is not registered")]
    MMNotRegistered,

    #[msg("Market maker is not active in registry")]
    MMNotActive,

    #[msg("Invalid Ed25519 signature")]
    InvalidSignature,

    #[msg("Quote nonce has already been used")]
    NonceAlreadyUsed,

    #[msg("Intent is not in pending status")]
    IntentNotPending,

    #[msg("Intent has not expired yet")]
    IntentNotExpired,

    #[msg("Intent has already expired")]
    IntentExpired,

    #[msg("Intent is not in a resolvable status")]
    IntentNotResolvable,

    #[msg("Intent is already disputed")]
    IntentAlreadyDisputed,

    #[msg("Only user or market maker can flag dispute")]
    UnauthorizedDispute,

    #[msg("Invalid percentage value")]
    InvalidPercentage,

    #[msg("Dispute reason too long")]
    DisputeReasonTooLong,

    #[msg("Only the designated market maker can fill this intent")]
    UnauthorizedFill,

    #[msg("Signing key mismatch")]
    SigningKeyMismatch,

    #[msg("Invalid vault address")]
    InvalidVault,
}

