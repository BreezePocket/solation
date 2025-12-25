use anchor_lang::prelude::*;

pub mod constants;
pub mod errors;
pub mod instructions;
pub mod state;

use instructions::*;
use state::*;

declare_id!("2KW4NXTSbeq5BJM39rsR1sR15Uf72perBkVVMH88ZGRm");

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

    // ===== Market Maker Instructions =====

    pub fn register_market_maker(ctx: Context<RegisterMarketMaker>) -> Result<()> {
        instructions::handle_register_market_maker(ctx)
    }

    pub fn initialize_vault(ctx: Context<InitializeVault>, asset_mint: Pubkey) -> Result<()> {
        instructions::handle_initialize_vault(ctx, asset_mint)
    }

    pub fn deposit_liquidity(ctx: Context<DepositLiquidity>, amount: u64) -> Result<()> {
        instructions::handle_deposit_liquidity(ctx, amount)
    }

    pub fn withdraw_liquidity(ctx: Context<WithdrawLiquidity>, amount: u64) -> Result<()> {
        instructions::handle_withdraw_liquidity(ctx, amount)
    }

    pub fn submit_quote(
        ctx: Context<SubmitQuote>,
        asset_mint: Pubkey,
        quote_mint: Pubkey,
        strategy: StrategyType,
        strikes: Vec<StrikeQuote>,
        expiry_timestamp: i64,
        min_size: u64,
        max_size: u64,
    ) -> Result<()> {
        instructions::handle_submit_quote(
            ctx,
            asset_mint,
            quote_mint,
            strategy,
            strikes,
            expiry_timestamp,
            min_size,
            max_size,
        )
    }

    pub fn update_quote(
        ctx: Context<UpdateQuote>,
        strikes: Option<Vec<StrikeQuote>>,
        expiry_timestamp: Option<i64>,
        min_size: Option<u64>,
        max_size: Option<u64>,
        active: Option<bool>,
    ) -> Result<()> {
        instructions::handle_update_quote(
            ctx,
            strikes,
            expiry_timestamp,
            min_size,
            max_size,
            active,
        )
    }

    // ===== Position Request Instructions (Two-Phase Commit) =====

    /// User requests a position - creates pending request for MM to approve
    pub fn request_position(
        ctx: Context<RequestPosition>,
        request_id: u64,
        strike_price: u64,
        contract_size: u64,
    ) -> Result<()> {
        instructions::handle_request_position(ctx, request_id, strike_price, contract_size)
    }

    /// MM confirms the request within 30 seconds - locks collateral and pays premium
    pub fn confirm_position(ctx: Context<ConfirmPosition>, position_id: u64) -> Result<()> {
        instructions::handle_confirm_position(ctx, position_id)
    }

    /// MM explicitly rejects the request
    pub fn reject_request(ctx: Context<RejectRequest>) -> Result<()> {
        instructions::handle_reject_request(ctx)
    }

    /// Anyone can cancel expired requests (after 30s timeout)
    pub fn cancel_expired_request(ctx: Context<CancelExpiredRequest>) -> Result<()> {
        instructions::handle_cancel_expired_request(ctx)
    }

    // ===== Settlement Instructions =====

    pub fn settle_position(ctx: Context<SettlePosition>) -> Result<()> {
        instructions::handle_settle_position(ctx)
    }
}
