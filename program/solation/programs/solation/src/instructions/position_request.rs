use crate::constants::*;
use crate::errors::ErrorCode;
use crate::state::*;
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

// ================================
// REQUEST POSITION (User initiates)
// ================================

#[derive(Accounts)]
#[instruction(request_id: u64, strike_price: u64)]
pub struct RequestPosition<'info> {
    #[account(
        seeds = [GLOBAL_STATE_SEED],
        bump = global_state.bump,
        constraint = !global_state.paused @ ErrorCode::ProtocolPaused
    )]
    pub global_state: Account<'info, GlobalState>,

    #[account(
        seeds = [MARKET_MAKER_SEED, market_maker.owner.as_ref()],
        bump = market_maker.bump,
        constraint = market_maker.active @ ErrorCode::MarketMakerNotActive
    )]
    pub market_maker: Account<'info, MarketMaker>,

    #[account(
        mut,
        seeds = [
            QUOTE_SEED,
            market_maker.key().as_ref(),
            quote.asset_mint.as_ref(),
            &[quote.strategy as u8],
            &quote.expiry_timestamp.to_le_bytes()
        ],
        bump = quote.bump,
        constraint = quote.active @ ErrorCode::QuoteNotActive
    )]
    pub quote: Account<'info, Quote>,

    #[account(
        seeds = [ASSET_CONFIG_SEED, asset_config.asset_mint.as_ref()],
        bump = asset_config.bump,
        constraint = asset_config.enabled @ ErrorCode::AssetNotEnabled
    )]
    pub asset_config: Account<'info, AssetConfig>,

    #[account(
        init,
        payer = user,
        space = PositionRequest::LEN,
        seeds = [POSITION_REQUEST_SEED, user.key().as_ref(), &request_id.to_le_bytes()],
        bump
    )]
    pub position_request: Account<'info, PositionRequest>,

    #[account(mut)]
    pub user: Signer<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handle_request_position(
    ctx: Context<RequestPosition>,
    request_id: u64,
    strike_price: u64,
    contract_size: u64,
) -> Result<()> {
    let clock = Clock::get()?;
    let quote = &ctx.accounts.quote;

    // Validate quote not expired
    require!(
        clock.unix_timestamp < quote.expiry_timestamp,
        ErrorCode::QuoteExpired
    );

    // Validate contract size
    require!(
        contract_size >= quote.min_size,
        ErrorCode::ContractSizeTooSmall
    );
    require!(
        contract_size <= quote.max_size,
        ErrorCode::ContractSizeTooLarge
    );

    // Find strike in quote
    let strike_quote = quote
        .strikes
        .iter()
        .find(|s| s.strike_price == strike_price)
        .ok_or(ErrorCode::StrikePriceNotFound)?;

    // Check available contracts
    require!(
        contract_size <= strike_quote.available_contracts,
        ErrorCode::InsufficientLiquidity
    );

    // Calculate premium
    let premium = strike_quote
        .premium_per_contract
        .checked_mul(contract_size)
        .ok_or(ErrorCode::MathOverflow)?;

    // Initialize position request
    let request = &mut ctx.accounts.position_request;
    request.request_id = request_id;
    request.user = ctx.accounts.user.key();
    request.market_maker = ctx.accounts.market_maker.key();
    request.quote = ctx.accounts.quote.key();
    request.strategy = quote.strategy;
    request.asset_mint = quote.asset_mint;
    request.quote_mint = quote.quote_mint;
    request.strike_price = strike_price;
    request.contract_size = contract_size;
    request.premium = premium;
    request.created_at = clock.unix_timestamp;
    request.expires_at = clock.unix_timestamp + MM_CONFIRMATION_WINDOW;
    request.status = RequestStatus::Pending;
    request.bump = ctx.bumps.position_request;

    msg!(
        "Position request created: {} (expires at {})",
        request_id,
        request.expires_at
    );

    Ok(())
}

// ================================
// CONFIRM POSITION (MM accepts)
// ================================

#[derive(Accounts)]
#[instruction(position_id: u64)]
pub struct ConfirmPosition<'info> {
    #[account(
        mut,
        seeds = [GLOBAL_STATE_SEED],
        bump = global_state.bump,
        constraint = !global_state.paused @ ErrorCode::ProtocolPaused
    )]
    pub global_state: Account<'info, GlobalState>,

    #[account(
        mut,
        seeds = [MARKET_MAKER_SEED, mm_owner.key().as_ref()],
        bump = market_maker.bump,
        constraint = market_maker.active @ ErrorCode::MarketMakerNotActive
    )]
    pub market_maker: Account<'info, MarketMaker>,

    #[account(
        mut,
        constraint = position_request.market_maker == market_maker.key() @ ErrorCode::UnauthorizedConfirmation,
        constraint = position_request.is_pending() @ ErrorCode::RequestNotPending
    )]
    pub position_request: Account<'info, PositionRequest>,

    #[account(
        mut,
        seeds = [
            QUOTE_SEED,
            market_maker.key().as_ref(),
            quote.asset_mint.as_ref(),
            &[quote.strategy as u8],
            &quote.expiry_timestamp.to_le_bytes()
        ],
        bump = quote.bump
    )]
    pub quote: Account<'info, Quote>,

    #[account(
        seeds = [ASSET_CONFIG_SEED, asset_config.asset_mint.as_ref()],
        bump = asset_config.bump
    )]
    pub asset_config: Account<'info, AssetConfig>,

    // Position account
    #[account(
        init,
        payer = mm_owner,
        space = Position::LEN,
        seeds = [POSITION_SEED, position_request.user.as_ref(), &position_id.to_le_bytes()],
        bump
    )]
    pub position: Account<'info, Position>,

    // User's vault (holds user's locked asset)
    #[account(
        init,
        payer = mm_owner,
        token::mint = user_asset_mint,
        token::authority = position_vault_authority,
        seeds = [POSITION_USER_VAULT_SEED, position.key().as_ref()],
        bump
    )]
    pub position_user_vault: Account<'info, TokenAccount>,

    // MM's vault (holds MM's locked asset)
    #[account(
        init,
        payer = mm_owner,
        token::mint = mm_asset_mint,
        token::authority = position_vault_authority,
        seeds = [POSITION_MM_VAULT_SEED, position.key().as_ref()],
        bump
    )]
    pub position_mm_vault: Account<'info, TokenAccount>,

    /// CHECK: PDA authority for position vaults
    #[account(
        seeds = [POSITION_SEED, position_request.user.as_ref(), &position_id.to_le_bytes()],
        bump
    )]
    pub position_vault_authority: AccountInfo<'info>,

    // Market maker's main vault account
    #[account(
        mut,
        seeds = [MM_VAULT_SEED, market_maker.key().as_ref(), mm_vault.asset_mint.as_ref()],
        bump = mm_vault.bump
    )]
    pub mm_vault: Account<'info, MarketMakerVault>,

    #[account(
        mut,
        token::mint = mm_asset_mint
    )]
    pub mm_vault_token_account: Account<'info, TokenAccount>,

    /// CHECK: PDA authority for MM vault
    #[account(
        seeds = [MM_VAULT_SEED, market_maker.key().as_ref(), mm_vault.asset_mint.as_ref()],
        bump = mm_vault.bump
    )]
    pub mm_vault_authority: AccountInfo<'info>,

    // MM's premium vault (USDC for premium payment)
    #[account(
        mut,
        seeds = [MM_VAULT_SEED, market_maker.key().as_ref(), premium_mint.key().as_ref()],
        bump = mm_premium_vault.bump
    )]
    pub mm_premium_vault: Account<'info, MarketMakerVault>,

    #[account(
        mut,
        token::mint = premium_mint
    )]
    pub mm_premium_vault_token_account: Account<'info, TokenAccount>,

    /// CHECK: PDA authority for MM premium vault
    #[account(
        seeds = [MM_VAULT_SEED, market_maker.key().as_ref(), premium_mint.key().as_ref()],
        bump = mm_premium_vault.bump
    )]
    pub mm_premium_vault_authority: AccountInfo<'info>,

    // User's token accounts
    #[account(
        mut,
        token::mint = user_asset_mint,
        token::authority = user
    )]
    pub user_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        token::mint = premium_mint,
        token::authority = user
    )]
    pub user_premium_account: Account<'info, TokenAccount>,

    // Mints
    pub user_asset_mint: Account<'info, Mint>,
    pub mm_asset_mint: Account<'info, Mint>,
    pub premium_mint: Account<'info, Mint>,

    /// CHECK: User who made the request
    pub user: AccountInfo<'info>,

    #[account(mut)]
    pub mm_owner: Signer<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn handle_confirm_position(ctx: Context<ConfirmPosition>, position_id: u64) -> Result<()> {
    let clock = Clock::get()?;
    let request = &ctx.accounts.position_request;

    // Check request hasn't expired
    require!(
        !request.is_expired(clock.unix_timestamp),
        ErrorCode::RequestExpired
    );

    // Get values from request
    let strike_price = request.strike_price;
    let contract_size = request.contract_size;
    let premium = request.premium;
    let strategy = request.strategy;

    // Execute based on strategy
    match strategy {
        StrategyType::CoveredCall => {
            execute_covered_call(&ctx, position_id, strike_price, contract_size, premium)?;
        }
        StrategyType::CashSecuredPut => {
            execute_cash_secured_put(&ctx, position_id, strike_price, contract_size, premium)?;
        }
    }

    // Initialize position
    let position = &mut ctx.accounts.position;
    position.position_id = position_id;
    position.user = ctx.accounts.position_request.user;
    position.market_maker = ctx.accounts.market_maker.key();
    position.strategy = strategy;
    position.asset_mint = ctx.accounts.asset_config.asset_mint;
    position.quote_mint = ctx.accounts.quote.quote_mint;
    position.strike_price = strike_price;
    position.premium_paid = premium;
    position.contract_size = contract_size;
    position.created_at = clock.unix_timestamp;
    position.expiry_timestamp = ctx.accounts.quote.expiry_timestamp;
    position.settlement_price = None;
    position.status = PositionStatus::Active;
    position.user_vault = ctx.accounts.position_user_vault.key();
    position.mm_vault_locked = ctx.accounts.position_mm_vault.key();
    position.bump = ctx.bumps.position;
    position.user_vault_bump = ctx.bumps.position_user_vault;
    position.mm_vault_bump = ctx.bumps.position_mm_vault;

    // Update request status
    ctx.accounts.position_request.status = RequestStatus::Accepted;

    // Update global state
    ctx.accounts.global_state.total_positions = ctx
        .accounts
        .global_state
        .total_positions
        .checked_add(1)
        .ok_or(ErrorCode::MathOverflow)?;
    ctx.accounts.global_state.total_volume = ctx
        .accounts
        .global_state
        .total_volume
        .checked_add(contract_size)
        .ok_or(ErrorCode::MathOverflow)?;

    // Update market maker stats
    ctx.accounts.market_maker.total_positions = ctx
        .accounts
        .market_maker
        .total_positions
        .checked_add(1)
        .ok_or(ErrorCode::MathOverflow)?;

    msg!("Position confirmed: {}", position.key());

    Ok(())
}

fn execute_covered_call(
    ctx: &Context<ConfirmPosition>,
    _position_id: u64,
    strike_price: u64,
    contract_size: u64,
    premium: u64,
) -> Result<()> {
    // Covered Call:
    // - User deposits underlying asset (contract_size)
    // - MM locks USDC (strike_price * contract_size)
    // - MM pays premium to user immediately

    let strike_amount = strike_price
        .checked_mul(contract_size)
        .ok_or(ErrorCode::MathOverflow)?
        .checked_div(10u64.pow(ctx.accounts.asset_config.decimals as u32))
        .ok_or(ErrorCode::MathOverflow)?;

    // Check MM has enough USDC liquidity
    require!(
        ctx.accounts.mm_vault.available_liquidity >= strike_amount,
        ErrorCode::InsufficientLiquidity
    );

    // Check MM has enough premium USDC
    require!(
        ctx.accounts.mm_premium_vault.available_liquidity >= premium,
        ErrorCode::InsufficientLiquidity
    );

    let cpi_program = ctx.accounts.token_program.to_account_info();

    // 1. Transfer user's underlying asset to position_user_vault
    let cpi_accounts_user = Transfer {
        from: ctx.accounts.user_token_account.to_account_info(),
        to: ctx.accounts.position_user_vault.to_account_info(),
        authority: ctx.accounts.user.to_account_info(),
    };
    token::transfer(
        CpiContext::new(cpi_program.clone(), cpi_accounts_user),
        contract_size,
    )?;

    // 2. Transfer MM's USDC to position_mm_vault
    let market_maker_key = ctx.accounts.market_maker.key();
    let asset_mint_key = ctx.accounts.mm_vault.asset_mint;
    let mm_vault_bump = ctx.accounts.mm_vault.bump;
    let mm_vault_seeds = &[
        MM_VAULT_SEED,
        market_maker_key.as_ref(),
        asset_mint_key.as_ref(),
        &[mm_vault_bump],
    ];
    let mm_vault_signer = &[&mm_vault_seeds[..]];

    let cpi_accounts_mm = Transfer {
        from: ctx.accounts.mm_vault_token_account.to_account_info(),
        to: ctx.accounts.position_mm_vault.to_account_info(),
        authority: ctx.accounts.mm_vault_authority.to_account_info(),
    };
    token::transfer(
        CpiContext::new_with_signer(cpi_program.clone(), cpi_accounts_mm, mm_vault_signer),
        strike_amount,
    )?;

    // 3. Transfer premium from MM to user
    let premium_mint_key = ctx.accounts.mm_premium_vault.asset_mint;
    let mm_premium_bump = ctx.accounts.mm_premium_vault.bump;
    let mm_premium_seeds = &[
        MM_VAULT_SEED,
        market_maker_key.as_ref(),
        premium_mint_key.as_ref(),
        &[mm_premium_bump],
    ];
    let mm_premium_signer = &[&mm_premium_seeds[..]];

    let cpi_accounts_premium = Transfer {
        from: ctx
            .accounts
            .mm_premium_vault_token_account
            .to_account_info(),
        to: ctx.accounts.user_premium_account.to_account_info(),
        authority: ctx.accounts.mm_premium_vault_authority.to_account_info(),
    };
    token::transfer(
        CpiContext::new_with_signer(cpi_program, cpi_accounts_premium, mm_premium_signer),
        premium,
    )?;

    msg!("Covered call executed - collateral locked, premium paid");

    Ok(())
}

fn execute_cash_secured_put(
    ctx: &Context<ConfirmPosition>,
    _position_id: u64,
    strike_price: u64,
    contract_size: u64,
    premium: u64,
) -> Result<()> {
    // Cash Secured Put:
    // - MM deposits underlying asset (contract_size)
    // - User deposits USDC (strike_price * contract_size)
    // - MM pays premium to user immediately

    let strike_amount = strike_price
        .checked_mul(contract_size)
        .ok_or(ErrorCode::MathOverflow)?
        .checked_div(10u64.pow(ctx.accounts.asset_config.decimals as u32))
        .ok_or(ErrorCode::MathOverflow)?;

    // Check MM has enough underlying asset
    require!(
        ctx.accounts.mm_vault.available_liquidity >= contract_size,
        ErrorCode::InsufficientLiquidity
    );

    // Check MM has enough premium USDC
    require!(
        ctx.accounts.mm_premium_vault.available_liquidity >= premium,
        ErrorCode::InsufficientLiquidity
    );

    let cpi_program = ctx.accounts.token_program.to_account_info();

    // 1. Transfer user's USDC to position_user_vault
    let cpi_accounts_user = Transfer {
        from: ctx.accounts.user_token_account.to_account_info(),
        to: ctx.accounts.position_user_vault.to_account_info(),
        authority: ctx.accounts.user.to_account_info(),
    };
    token::transfer(
        CpiContext::new(cpi_program.clone(), cpi_accounts_user),
        strike_amount,
    )?;

    // 2. Transfer MM's underlying asset to position_mm_vault
    let market_maker_key = ctx.accounts.market_maker.key();
    let asset_mint_key = ctx.accounts.mm_vault.asset_mint;
    let mm_vault_bump = ctx.accounts.mm_vault.bump;
    let mm_vault_seeds = &[
        MM_VAULT_SEED,
        market_maker_key.as_ref(),
        asset_mint_key.as_ref(),
        &[mm_vault_bump],
    ];
    let mm_vault_signer = &[&mm_vault_seeds[..]];

    let cpi_accounts_mm = Transfer {
        from: ctx.accounts.mm_vault_token_account.to_account_info(),
        to: ctx.accounts.position_mm_vault.to_account_info(),
        authority: ctx.accounts.mm_vault_authority.to_account_info(),
    };
    token::transfer(
        CpiContext::new_with_signer(cpi_program.clone(), cpi_accounts_mm, mm_vault_signer),
        contract_size,
    )?;

    // 3. Transfer premium from MM to user
    let premium_mint_key = ctx.accounts.mm_premium_vault.asset_mint;
    let mm_premium_bump = ctx.accounts.mm_premium_vault.bump;
    let mm_premium_seeds = &[
        MM_VAULT_SEED,
        market_maker_key.as_ref(),
        premium_mint_key.as_ref(),
        &[mm_premium_bump],
    ];
    let mm_premium_signer = &[&mm_premium_seeds[..]];

    let cpi_accounts_premium = Transfer {
        from: ctx
            .accounts
            .mm_premium_vault_token_account
            .to_account_info(),
        to: ctx.accounts.user_premium_account.to_account_info(),
        authority: ctx.accounts.mm_premium_vault_authority.to_account_info(),
    };
    token::transfer(
        CpiContext::new_with_signer(cpi_program, cpi_accounts_premium, mm_premium_signer),
        premium,
    )?;

    msg!("Cash secured put executed - collateral locked, premium paid");

    Ok(())
}

// ================================
// REJECT REQUEST (MM rejects)
// ================================

#[derive(Accounts)]
pub struct RejectRequest<'info> {
    #[account(
        seeds = [MARKET_MAKER_SEED, mm_owner.key().as_ref()],
        bump = market_maker.bump
    )]
    pub market_maker: Account<'info, MarketMaker>,

    #[account(
        mut,
        close = user,
        constraint = position_request.market_maker == market_maker.key() @ ErrorCode::UnauthorizedConfirmation,
        constraint = position_request.is_pending() @ ErrorCode::RequestNotPending
    )]
    pub position_request: Account<'info, PositionRequest>,

    /// CHECK: User who made the request (receives rent refund)
    #[account(mut)]
    pub user: AccountInfo<'info>,

    pub mm_owner: Signer<'info>,
}

pub fn handle_reject_request(_ctx: Context<RejectRequest>) -> Result<()> {
    // The request is closed and rent returned to user
    // Status is implicitly "Rejected" since account is closed
    msg!("Position request rejected by MM");
    Ok(())
}

// ================================
// CANCEL EXPIRED REQUEST (Anyone)
// ================================

#[derive(Accounts)]
pub struct CancelExpiredRequest<'info> {
    #[account(
        mut,
        close = user,
        constraint = position_request.is_pending() @ ErrorCode::RequestNotPending
    )]
    pub position_request: Account<'info, PositionRequest>,

    /// CHECK: User who made the request (receives rent refund)
    #[account(
        mut,
        constraint = user.key() == position_request.user
    )]
    pub user: AccountInfo<'info>,

    /// Anyone can call this after expiry
    pub caller: Signer<'info>,
}

pub fn handle_cancel_expired_request(ctx: Context<CancelExpiredRequest>) -> Result<()> {
    let clock = Clock::get()?;

    // Verify request has expired
    require!(
        ctx.accounts
            .position_request
            .is_expired(clock.unix_timestamp),
        ErrorCode::RequestNotExpired
    );

    // The request is closed and rent returned to user
    msg!("Expired position request cancelled");
    Ok(())
}
