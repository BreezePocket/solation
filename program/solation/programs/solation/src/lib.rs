use anchor_lang::prelude::*;

pub mod constants;
pub mod errors;
pub mod instructions;
pub mod state;
pub mod utils;

use instructions::*;
use state::*;

declare_id!("4XkfZ5hHr43pSZBioix3ps8Y8UR1ghN6fjP1zccEFYXQ");

#[program]
pub mod solation {
    use super::*;

    // ===== Admin Instructions =====

    pub fn initialize_global_state(
        ctx: Context<InitializeGlobalState>,
        protocol_fee_bps: u16,
    ) -> Result<()> {
        instructions::handle_initialize_global_state(ctx, protocol_fee_bps)
    }

    pub fn update_global_state(
        ctx: Context<UpdateGlobalState>,
        new_authority: Option<Pubkey>,
        new_treasury: Option<Pubkey>,
        new_fee_bps: Option<u16>,
        paused: Option<bool>,
    ) -> Result<()> {
        instructions::handle_update_global_state(
            ctx,
            new_authority,
            new_treasury,
            new_fee_bps,
            paused,
        )
    }

    pub fn add_asset(
        ctx: Context<AddAsset>,
        asset_mint: Pubkey,
        quote_mint: Pubkey,
        pyth_feed_id: [u8; 32],
        min_strike_percentage: u16,
        max_strike_percentage: u16,
        min_expiry_seconds: i64,
        max_expiry_seconds: i64,
        decimals: u8,
    ) -> Result<()> {
        instructions::handle_add_asset(
            ctx,
            asset_mint,
            quote_mint,
            pyth_feed_id,
            min_strike_percentage,
            max_strike_percentage,
            min_expiry_seconds,
            max_expiry_seconds,
            decimals,
        )
    }

    pub fn update_asset(
        ctx: Context<UpdateAsset>,
        enabled: Option<bool>,
        min_strike_percentage: Option<u16>,
        max_strike_percentage: Option<u16>,
        min_expiry_seconds: Option<i64>,
        max_expiry_seconds: Option<i64>,
    ) -> Result<()> {
        instructions::handle_update_asset(
            ctx,
            enabled,
            min_strike_percentage,
            max_strike_percentage,
            min_expiry_seconds,
            max_expiry_seconds,
        )
    }

    // ===== Market Maker Registration (Off-Chain RFQ) =====

    /// MM registers with their Ed25519 signing key
    pub fn register_mm(ctx: Context<RegisterMM>, signing_key: Pubkey) -> Result<()> {
        instructions::handle_register_mm(ctx, signing_key)
    }

    /// MM updates their signing key
    pub fn update_mm_signing_key(
        ctx: Context<UpdateMMSigningKey>,
        new_signing_key: Pubkey,
    ) -> Result<()> {
        instructions::handle_update_mm_signing_key(ctx, new_signing_key)
    }

    // ===== Intent Lifecycle (Off-Chain RFQ) =====

    /// User submits intent with MM's signed quote
    pub fn submit_intent(ctx: Context<SubmitIntent>, params: SubmitIntentParams) -> Result<()> {
        instructions::handle_submit_intent(ctx, params)
    }

    /// MM fills the intent (creates Position, pays premium)
    pub fn fill_intent(ctx: Context<FillIntent>) -> Result<()> {
        instructions::handle_fill_intent(ctx)
    }

    /// User cancels unfilled intent (reclaims escrow)
    pub fn cancel_intent(ctx: Context<CancelIntent>) -> Result<()> {
        instructions::handle_cancel_intent(ctx)
    }

    /// Anyone can cleanup expired intents
    pub fn expire_intent(ctx: Context<ExpireIntent>) -> Result<()> {
        instructions::handle_expire_intent(ctx)
    }

    /// User or MM flags intent for dispute
    pub fn flag_dispute(ctx: Context<FlagDispute>, reason: String) -> Result<()> {
        instructions::handle_flag_dispute(ctx, reason)
    }

    // ===== Dispute Resolution (Owner Override) =====

    /// 1. MUTUAL_UNWIND: Return all funds to original parties
    pub fn mutual_unwind(ctx: Context<MutualUnwindIntent>, reason: String) -> Result<()> {
        instructions::handle_mutual_unwind(ctx, reason)
    }

    /// 2. FORCE_CONTINUE: Force-create position as if MM had filled
    pub fn force_continue(
        ctx: Context<ForceContinueIntent>,
        reason: String,
        pay_premium: bool,
    ) -> Result<()> {
        instructions::handle_force_continue(ctx, reason, pay_premium)
    }

    /// 3. FORCE_SETTLE_NOW: Settle immediately at specified price/split
    pub fn force_settle_now(
        ctx: Context<ForceSettleNowIntent>,
        settlement_price: u64,
        user_payout_bps: u16,
        reason: String,
    ) -> Result<()> {
        instructions::handle_force_settle_now(ctx, settlement_price, user_payout_bps, reason)
    }

    /// 4. ESCROW_TO_TREASURY: Move funds to treasury for manual distribution
    pub fn escrow_to_treasury(ctx: Context<EscrowToTreasuryIntent>, reason: String) -> Result<()> {
        instructions::handle_escrow_to_treasury(ctx, reason)
    }

    /// 5. PROPORTIONAL_SPLIT: Split escrow by percentage
    pub fn proportional_split(
        ctx: Context<ProportionalSplitIntent>,
        user_bps: u16,
        reason: String,
    ) -> Result<()> {
        instructions::handle_proportional_split(ctx, user_bps, reason)
    }

    /// 6. EMERGENCY_SHUTDOWN: Global pause, prepare for mass unwind
    pub fn emergency_shutdown(ctx: Context<TriggerEmergencyShutdown>, reason: String) -> Result<()> {
        instructions::handle_emergency_shutdown(ctx, reason)
    }

    // ===== Settlement =====

    pub fn settle_position(ctx: Context<SettlePosition>) -> Result<()> {
        instructions::handle_settle_position(ctx)
    }
}
