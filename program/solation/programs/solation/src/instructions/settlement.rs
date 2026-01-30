use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};
use pyth_solana_receiver_sdk::price_update::PriceUpdateV2;
use crate::state::*;
use crate::constants::*;
use crate::errors::ErrorCode;

/// Settle a position at expiry using Pyth oracle price
#[derive(Accounts)]
pub struct SettlePosition<'info> {
    /// Anyone can call settle (permissionless settlement)
    pub settler: Signer<'info>,

    #[account(
        mut,
        constraint = position.status == PositionStatus::Active @ ErrorCode::PositionNotActive
    )]
    pub position: Account<'info, Position>,

    #[account(
        seeds = [ASSET_CONFIG_SEED, asset_config.asset_mint.as_ref()],
        bump = asset_config.bump
    )]
    pub asset_config: Account<'info, AssetConfig>,

    /// MM's registry (for stats tracking)
    #[account(
        mut,
        seeds = [MM_REGISTRY_SEED, position.market_maker.as_ref()],
        bump = mm_registry.bump
    )]
    pub mm_registry: Account<'info, MMRegistry>,

    /// Position's user vault (user's locked collateral)
    #[account(
        mut,
        constraint = position_user_vault.key() == position.user_vault @ ErrorCode::InvalidVault
    )]
    pub position_user_vault: Account<'info, TokenAccount>,

    /// Position's MM vault (MM's locked collateral if any)
    #[account(mut)]
    pub position_mm_vault: Account<'info, TokenAccount>,

    /// CHECK: PDA authority for position vaults
    #[account(
        seeds = [POSITION_SEED, position.user.as_ref(), &position.position_id.to_le_bytes()],
        bump = position.bump
    )]
    pub position_authority: AccountInfo<'info>,

    /// User's destination token account
    #[account(
        mut,
        constraint = user_destination.owner == position.user
    )]
    pub user_destination: Account<'info, TokenAccount>,

    /// MM's destination token account  
    #[account(
        mut,
        constraint = mm_destination.owner == position.market_maker
    )]
    pub mm_destination: Account<'info, TokenAccount>,

    /// Pyth price feed
    /// CHECK: Validated by Pyth SDK
    pub price_update: AccountInfo<'info>,

    pub token_program: Program<'info, Token>,
}

pub fn handle_settle_position(ctx: Context<SettlePosition>) -> Result<()> {
    let clock = Clock::get()?;

    // Check position has expired
    require!(
        clock.unix_timestamp >= ctx.accounts.position.expiry_timestamp,
        ErrorCode::PositionNotExpired
    );

    // Load Pyth price and validate
    let settlement_price = get_pyth_price(
        &ctx.accounts.price_update,
        &ctx.accounts.asset_config.pyth_feed_id,
        clock.unix_timestamp,
    )?;

    msg!("Settlement price: {}", settlement_price);
    msg!("Strike price: {}", ctx.accounts.position.strike_price);

    // Store settlement price
    let position = &mut ctx.accounts.position;
    position.settlement_price = Some(settlement_price);

    let strike_price = position.strike_price;
    let contract_size = position.contract_size;
    let strategy = position.strategy;

    // Calculate payout based on strategy and ITM/OTM
    let (user_amount, mm_amount, status) = calculate_settlement(
        strategy,
        settlement_price,
        strike_price,
        contract_size,
        ctx.accounts.position_user_vault.amount,
    );

    // Prepare PDA signer
    let position_seeds = &[
        POSITION_SEED,
        position.user.as_ref(),
        &position.position_id.to_le_bytes(),
        &[position.bump],
    ];
    let signer = &[&position_seeds[..]];

    // Transfer user's share
    if user_amount > 0 {
        let cpi_accounts = Transfer {
            from: ctx.accounts.position_user_vault.to_account_info(),
            to: ctx.accounts.user_destination.to_account_info(),
            authority: ctx.accounts.position_authority.to_account_info(),
        };
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                cpi_accounts,
                signer,
            ),
            user_amount,
        )?;
    }

    // Transfer MM's share
    if mm_amount > 0 {
        let cpi_accounts = Transfer {
            from: ctx.accounts.position_user_vault.to_account_info(),
            to: ctx.accounts.mm_destination.to_account_info(),
            authority: ctx.accounts.position_authority.to_account_info(),
        };
        token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                cpi_accounts,
                signer,
            ),
            mm_amount,
        )?;
    }

    // Update position status
    let position = &mut ctx.accounts.position;
    position.status = status;

    // Update MM stats
    let mm_registry = &mut ctx.accounts.mm_registry;
    mm_registry.total_intents_filled = mm_registry.total_intents_filled.saturating_add(1);

    msg!("Position {} settled. User: {}, MM: {}", 
         position.position_id, user_amount, mm_amount);

    Ok(())
}

/// Get Pyth price with validation
fn get_pyth_price(
    price_update_account: &AccountInfo,
    expected_feed_id: &[u8; 32],
    current_timestamp: i64,
) -> Result<u64> {
    let price_update_data = price_update_account.try_borrow_data()
        .map_err(|_| ErrorCode::PriceTooStale)?;

    let price_update = PriceUpdateV2::try_from_slice(&price_update_data)
        .map_err(|_| ErrorCode::PriceTooStale)?;

    // Get price
    let price = price_update.get_price_unchecked(expected_feed_id)
        .map_err(|_| ErrorCode::PythFeedIdMismatch)?;

    // Staleness check
    let price_timestamp = price_update.price_message.publish_time;
    require!(
        current_timestamp - price_timestamp < PYTH_STALENESS_THRESHOLD as i64,
        ErrorCode::PriceTooStale
    );

    // Verify feed ID
    require!(
        price_update.price_message.feed_id == *expected_feed_id,
        ErrorCode::PythFeedIdMismatch
    );

    // Convert to u64 (handle negative prices)
    Ok(price.price.unsigned_abs())
}

/// Calculate settlement amounts based on strategy
fn calculate_settlement(
    strategy: StrategyType,
    settlement_price: u64,
    strike_price: u64,
    _contract_size: u64,
    vault_amount: u64,
) -> (u64, u64, PositionStatus) {
    match strategy {
        StrategyType::CoveredCall => {
            if settlement_price > strike_price {
                // ITM: MM exercises, gets the difference value
                // User gets strike price worth
                // MM gets the rest (upside)
                let strike_value = vault_amount.saturating_mul(strike_price) / settlement_price;
                let mm_gain = vault_amount.saturating_sub(strike_value);
                (strike_value, mm_gain, PositionStatus::SettledITM)
            } else {
                // OTM: Expires worthless, user keeps collateral, MM keeps premium
                (vault_amount, 0, PositionStatus::SettledOTM)
            }
        }
        StrategyType::CashSecuredPut => {
            if settlement_price < strike_price {
                // ITM: User must buy at strike, MM delivers asset value
                // MM gets the collateral (user's USDC at strike)
                // User gets underlying value worth of USDC
                let user_value = vault_amount.saturating_mul(settlement_price) / strike_price;
                let mm_gain = vault_amount.saturating_sub(user_value);
                (user_value, mm_gain, PositionStatus::SettledITM)
            } else {
                // OTM: Expires worthless, user keeps USDC, MM keeps premium
                (vault_amount, 0, PositionStatus::SettledOTM)
            }
        }
    }
}
