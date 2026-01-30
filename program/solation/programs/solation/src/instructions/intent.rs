use anchor_lang::prelude::*;
use anchor_lang::solana_program::sysvar::instructions::ID as INSTRUCTIONS_SYSVAR_ID;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

use crate::constants::*;
use crate::errors::ErrorCode;
use crate::state::*;
use crate::utils::ed25519_verify::{construct_quote_message, verify_ed25519_signature};

// ===== Events =====

#[event]
pub struct IntentCreated {
    pub intent_id: u64,
    pub user: Pubkey,
    pub market_maker: Pubkey,
    pub asset_mint: Pubkey,
    pub strategy: StrategyType,
    pub strike_price: u64,
    pub premium: u64,
    pub contract_size: u64,
    pub fill_deadline: i64,
}

#[event]
pub struct IntentFilled {
    pub intent_id: u64,
    pub position_id: u64,
    pub market_maker: Pubkey,
    pub user: Pubkey,
}

#[event]
pub struct IntentCancelled {
    pub intent_id: u64,
    pub user: Pubkey,
}

#[event]
pub struct IntentExpired {
    pub intent_id: u64,
    pub market_maker: Pubkey,
}

#[event]
pub struct DisputeFlagged {
    pub intent_id: u64,
    pub flagged_by: Pubkey,
    pub reason: String,
}

// ===== Register MM =====

#[derive(Accounts)]
pub struct RegisterMM<'info> {
    #[account(mut)]
    pub owner: Signer<'info>,

    #[account(
        init,
        payer = owner,
        space = MMRegistry::LEN,
        seeds = [MM_REGISTRY_SEED, owner.key().as_ref()],
        bump
    )]
    pub mm_registry: Account<'info, MMRegistry>,

    #[account(
        init,
        payer = owner,
        space = NonceTracker::LEN,
        seeds = [NONCE_TRACKER_SEED, owner.key().as_ref()],
        bump
    )]
    pub nonce_tracker: Account<'info, NonceTracker>,

    pub system_program: Program<'info, System>,
}

pub fn handle_register_mm(
    ctx: Context<RegisterMM>,
    signing_key: Pubkey,
) -> Result<()> {
    let clock = Clock::get()?;
    
    let mm_registry = &mut ctx.accounts.mm_registry;
    mm_registry.owner = ctx.accounts.owner.key();
    mm_registry.signing_key = signing_key;
    mm_registry.active = true;
    mm_registry.total_intents_filled = 0;
    mm_registry.total_intents_expired = 0;
    mm_registry.total_volume = 0;
    mm_registry.reputation_score = 100; // Start with base score
    mm_registry.last_active = clock.unix_timestamp;
    mm_registry.registered_at = clock.unix_timestamp;
    mm_registry.bump = ctx.bumps.mm_registry;

    let nonce_tracker = &mut ctx.accounts.nonce_tracker;
    nonce_tracker.market_maker = ctx.accounts.owner.key();
    nonce_tracker.base_nonce = 0;
    nonce_tracker.used_bitmap = [0; 32];
    nonce_tracker.bump = ctx.bumps.nonce_tracker;

    Ok(())
}

// ===== Update MM Signing Key =====

#[derive(Accounts)]
pub struct UpdateMMSigningKey<'info> {
    pub owner: Signer<'info>,

    #[account(
        mut,
        seeds = [MM_REGISTRY_SEED, owner.key().as_ref()],
        bump = mm_registry.bump,
        constraint = mm_registry.owner == owner.key() @ ErrorCode::Unauthorized
    )]
    pub mm_registry: Account<'info, MMRegistry>,
}

pub fn handle_update_mm_signing_key(
    ctx: Context<UpdateMMSigningKey>,
    new_signing_key: Pubkey,
) -> Result<()> {
    let mm_registry = &mut ctx.accounts.mm_registry;
    mm_registry.signing_key = new_signing_key;
    Ok(())
}

// ===== Submit Intent =====

#[derive(Accounts)]
#[instruction(intent_id: u64)]
pub struct SubmitIntent<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        seeds = [GLOBAL_STATE_SEED],
        bump = global_state.bump,
        constraint = !global_state.paused @ ErrorCode::ProtocolPaused
    )]
    pub global_state: Account<'info, GlobalState>,

    /// The market maker's registry
    #[account(
        seeds = [MM_REGISTRY_SEED, mm_registry.owner.as_ref()],
        bump = mm_registry.bump,
        constraint = mm_registry.active @ ErrorCode::MMNotActive
    )]
    pub mm_registry: Account<'info, MMRegistry>,

    /// Nonce tracker for the MM
    #[account(
        mut,
        seeds = [NONCE_TRACKER_SEED, mm_registry.owner.as_ref()],
        bump = nonce_tracker.bump
    )]
    pub nonce_tracker: Account<'info, NonceTracker>,

    /// The intent account to create
    #[account(
        init,
        payer = user,
        space = Intent::LEN,
        seeds = [INTENT_SEED, user.key().as_ref(), &intent_id.to_le_bytes()],
        bump
    )]
    pub intent: Account<'info, Intent>,

    /// User's escrow token account (PDA)
    #[account(
        init,
        payer = user,
        token::mint = quote_mint,
        token::authority = intent,
        seeds = [USER_ESCROW_SEED, intent.key().as_ref()],
        bump
    )]
    pub user_escrow: Account<'info, TokenAccount>,

    /// User's source token account
    #[account(
        mut,
        constraint = user_token_account.owner == user.key()
    )]
    pub user_token_account: Account<'info, TokenAccount>,

    /// Quote mint (USDC)
    pub quote_mint: Account<'info, anchor_spl::token::Mint>,

    /// Instructions sysvar for Ed25519 signature verification
    /// CHECK: This is the instructions sysvar
    #[account(address = INSTRUCTIONS_SYSVAR_ID)]
    pub instructions_sysvar: AccountInfo<'info>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

/// Parameters for submitting an intent
#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct SubmitIntentParams {
    pub intent_id: u64,
    pub asset_mint: Pubkey,
    pub quote_mint: Pubkey,
    pub strategy: StrategyType,
    pub strike_price: u64,
    pub premium_per_contract: u64,
    pub contract_size: u64,
    pub quote_expiry: i64,
    pub quote_nonce: u64,
    pub mm_signature: [u8; 64],
    /// Index of Ed25519Program instruction in the transaction (typically 0)
    pub ed25519_instruction_index: u8,
}

pub fn handle_submit_intent(
    ctx: Context<SubmitIntent>,
    params: SubmitIntentParams,
) -> Result<()> {
    let clock = Clock::get()?;

    // 1. Verify quote hasn't expired
    require!(params.quote_expiry > clock.unix_timestamp, ErrorCode::QuoteExpired);

    // 2. Check nonce not reused
    let nonce_tracker = &mut ctx.accounts.nonce_tracker;
    require!(
        !nonce_tracker.is_used(params.quote_nonce),
        ErrorCode::NonceAlreadyUsed
    );
    nonce_tracker.mark_used(params.quote_nonce)?;

    // 3. Verify Ed25519 signature
    let expected_message = construct_quote_message(
        &params.asset_mint,
        &params.quote_mint,
        params.strategy,
        params.strike_price,
        params.premium_per_contract,
        params.contract_size,
        params.quote_expiry,
        params.quote_nonce,
    );

    verify_ed25519_signature(
        &ctx.accounts.instructions_sysvar,
        &ctx.accounts.mm_registry.signing_key,
        &expected_message,
        params.ed25519_instruction_index,
    )?;
    
    // 4. Calculate escrow amount based on strategy
    let escrow_amount = calculate_escrow_amount(
        params.strategy,
        params.strike_price,
        params.contract_size,
    );

    // 5. Transfer user funds to escrow
    let cpi_accounts = Transfer {
        from: ctx.accounts.user_token_account.to_account_info(),
        to: ctx.accounts.user_escrow.to_account_info(),
        authority: ctx.accounts.user.to_account_info(),
    };
    let cpi_program = ctx.accounts.token_program.to_account_info();
    let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
    token::transfer(cpi_ctx, escrow_amount)?;

    // 6. Create Intent account
    let intent = &mut ctx.accounts.intent;
    intent.intent_id = params.intent_id;
    intent.user = ctx.accounts.user.key();
    intent.market_maker = ctx.accounts.mm_registry.owner;
    intent.asset_mint = params.asset_mint;
    intent.quote_mint = params.quote_mint;
    intent.strategy = params.strategy;
    intent.strike_price = params.strike_price;
    intent.premium_per_contract = params.premium_per_contract;
    intent.contract_size = params.contract_size;
    intent.quote_expiry = params.quote_expiry;
    intent.quote_signature = params.mm_signature;
    intent.quote_nonce = params.quote_nonce;
    intent.user_escrow = ctx.accounts.user_escrow.key();
    intent.escrow_amount = escrow_amount;
    intent.created_at = clock.unix_timestamp;
    intent.fill_deadline = clock.unix_timestamp + INTENT_FILL_TIMEOUT;
    intent.disputed_by = None;
    intent.dispute_reason = None;
    intent.status = IntentStatus::Pending;
    intent.bump = ctx.bumps.intent;

    emit!(IntentCreated {
        intent_id: intent.intent_id,
        user: intent.user,
        market_maker: intent.market_maker,
        asset_mint: intent.asset_mint,
        strategy: intent.strategy,
        strike_price: intent.strike_price,
        premium: intent.calculate_total_premium(),
        contract_size: intent.contract_size,
        fill_deadline: intent.fill_deadline,
    });

    Ok(())
}

/// Calculate escrow amount based on strategy
fn calculate_escrow_amount(
    strategy: StrategyType,
    strike_price: u64,
    contract_size: u64,
) -> u64 {
    match strategy {
        // Covered Call: User deposits the underlying asset
        // For simplicity, we'll use contract_size as the escrow
        StrategyType::CoveredCall => contract_size,
        // Cash Secured Put: User deposits strike_price * contract_size
        StrategyType::CashSecuredPut => {
            strike_price.saturating_mul(contract_size) / 1_000_000 // Adjust for decimals
        }
    }
}

// ===== Fill Intent =====

#[derive(Accounts)]
pub struct FillIntent<'info> {
    #[account(mut)]
    pub market_maker: Signer<'info>,

    #[account(
        seeds = [GLOBAL_STATE_SEED],
        bump = global_state.bump,
        constraint = !global_state.paused @ ErrorCode::ProtocolPaused
    )]
    pub global_state: Account<'info, GlobalState>,

    #[account(
        mut,
        constraint = intent.is_pending() @ ErrorCode::IntentNotPending,
        constraint = intent.market_maker == market_maker.key() @ ErrorCode::UnauthorizedFill
    )]
    pub intent: Account<'info, Intent>,

    #[account(
        mut,
        seeds = [MM_REGISTRY_SEED, market_maker.key().as_ref()],
        bump = mm_registry.bump,
        constraint = mm_registry.active @ ErrorCode::MMNotActive
    )]
    pub mm_registry: Account<'info, MMRegistry>,

    /// User's escrow token account
    #[account(
        mut,
        seeds = [USER_ESCROW_SEED, intent.key().as_ref()],
        bump
    )]
    pub user_escrow: Account<'info, TokenAccount>,

    /// User's token account to receive premium
    #[account(
        mut,
        constraint = user_token_account.owner == intent.user
    )]
    pub user_token_account: Account<'info, TokenAccount>,

    /// MM's token account to pay premium from
    #[account(
        mut,
        constraint = mm_token_account.owner == market_maker.key()
    )]
    pub mm_token_account: Account<'info, TokenAccount>,

    /// Position account to create
    #[account(
        init,
        payer = market_maker,
        space = Position::LEN,
        seeds = [POSITION_SEED, intent.user.as_ref(), &intent.intent_id.to_le_bytes()],
        bump
    )]
    pub position: Account<'info, Position>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn handle_fill_intent(ctx: Context<FillIntent>) -> Result<()> {
    let clock = Clock::get()?;
    let intent = &ctx.accounts.intent;

    // 1. Verify intent hasn't expired
    require!(
        clock.unix_timestamp <= intent.fill_deadline,
        ErrorCode::IntentExpired
    );

    // 2. Calculate premium
    let total_premium = intent.calculate_total_premium();

    // 3. Transfer premium from MM to user
    let cpi_accounts = Transfer {
        from: ctx.accounts.mm_token_account.to_account_info(),
        to: ctx.accounts.user_token_account.to_account_info(),
        authority: ctx.accounts.market_maker.to_account_info(),
    };
    let cpi_program = ctx.accounts.token_program.to_account_info();
    let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
    token::transfer(cpi_ctx, total_premium)?;

    // 4. Return user escrow (the collateral stays with intent for now, 
    // or we can transfer to a position-specific vault)
    // For simplicity, we leave escrow in place as position collateral
    // In production, you'd transfer to position-specific vaults

    // 5. Create Position
    let position = &mut ctx.accounts.position;
    position.position_id = intent.intent_id;
    position.user = intent.user;
    position.market_maker = intent.market_maker;
    position.strategy = intent.strategy;
    position.asset_mint = intent.asset_mint;
    position.quote_mint = intent.quote_mint;
    position.strike_price = intent.strike_price;
    position.premium_paid = total_premium;
    position.contract_size = intent.contract_size;
    position.created_at = clock.unix_timestamp;
    position.expiry_timestamp = intent.quote_expiry;
    position.settlement_price = None;
    position.status = PositionStatus::Active;
    position.user_vault = intent.user_escrow; // Reuse escrow as user vault
    position.mm_vault_locked = ctx.accounts.mm_token_account.key(); // Track MM account
    position.bump = ctx.bumps.position;
    position.user_vault_bump = 0; // Not using separate vault
    position.mm_vault_bump = 0;

    // 6. Update MM stats
    let mm_registry = &mut ctx.accounts.mm_registry;
    mm_registry.record_fill(intent.contract_size, clock.unix_timestamp);

    // 7. Update intent status
    let intent = &mut ctx.accounts.intent;
    intent.status = IntentStatus::Filled;

    emit!(IntentFilled {
        intent_id: intent.intent_id,
        position_id: position.position_id,
        market_maker: ctx.accounts.market_maker.key(),
        user: intent.user,
    });

    Ok(())
}

// ===== Cancel Intent =====

#[derive(Accounts)]
pub struct CancelIntent<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        mut,
        seeds = [INTENT_SEED, user.key().as_ref(), &intent.intent_id.to_le_bytes()],
        bump = intent.bump,
        constraint = intent.user == user.key() @ ErrorCode::Unauthorized,
        constraint = intent.is_pending() @ ErrorCode::IntentNotPending
    )]
    pub intent: Account<'info, Intent>,

    /// User's escrow token account
    #[account(
        mut,
        seeds = [USER_ESCROW_SEED, intent.key().as_ref()],
        bump
    )]
    pub user_escrow: Account<'info, TokenAccount>,

    /// User's destination token account
    #[account(mut)]
    pub user_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

pub fn handle_cancel_intent(ctx: Context<CancelIntent>) -> Result<()> {
    let intent = &ctx.accounts.intent;
    
    // Return escrow to user
    let escrow_amount = intent.escrow_amount;
    let intent_key = intent.key();
    let seeds = &[
        USER_ESCROW_SEED,
        intent_key.as_ref(),
        &[ctx.bumps.user_escrow],
    ];
    let signer_seeds = &[&seeds[..]];

    let cpi_accounts = Transfer {
        from: ctx.accounts.user_escrow.to_account_info(),
        to: ctx.accounts.user_token_account.to_account_info(),
        authority: ctx.accounts.intent.to_account_info(),
    };
    let cpi_program = ctx.accounts.token_program.to_account_info();
    let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer_seeds);
    token::transfer(cpi_ctx, escrow_amount)?;

    // Update status
    let intent = &mut ctx.accounts.intent;
    intent.status = IntentStatus::Cancelled;

    emit!(IntentCancelled {
        intent_id: intent.intent_id,
        user: intent.user,
    });

    Ok(())
}

// ===== Expire Intent =====

#[derive(Accounts)]
pub struct ExpireIntent<'info> {
    /// Anyone can call this after deadline
    pub caller: Signer<'info>,

    #[account(
        mut,
        constraint = intent.is_pending() @ ErrorCode::IntentNotPending
    )]
    pub intent: Account<'info, Intent>,

    #[account(
        mut,
        seeds = [MM_REGISTRY_SEED, intent.market_maker.as_ref()],
        bump = mm_registry.bump
    )]
    pub mm_registry: Account<'info, MMRegistry>,

    /// User's escrow token account
    #[account(
        mut,
        seeds = [USER_ESCROW_SEED, intent.key().as_ref()],
        bump
    )]
    pub user_escrow: Account<'info, TokenAccount>,

    /// User's destination token account
    #[account(
        mut,
        constraint = user_token_account.owner == intent.user
    )]
    pub user_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

pub fn handle_expire_intent(ctx: Context<ExpireIntent>) -> Result<()> {
    let clock = Clock::get()?;
    let intent = &ctx.accounts.intent;

    // Verify intent has expired
    require!(
        clock.unix_timestamp > intent.fill_deadline,
        ErrorCode::IntentNotExpired
    );

    // Return escrow to user
    let escrow_amount = intent.escrow_amount;
    let intent_key = intent.key();
    let seeds = &[
        USER_ESCROW_SEED,
        intent_key.as_ref(),
        &[ctx.bumps.user_escrow],
    ];
    let signer_seeds = &[&seeds[..]];

    let cpi_accounts = Transfer {
        from: ctx.accounts.user_escrow.to_account_info(),
        to: ctx.accounts.user_token_account.to_account_info(),
        authority: ctx.accounts.intent.to_account_info(),
    };
    let cpi_program = ctx.accounts.token_program.to_account_info();
    let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer_seeds);
    token::transfer(cpi_ctx, escrow_amount)?;

    // Penalize MM reputation
    let mm_registry = &mut ctx.accounts.mm_registry;
    mm_registry.record_expire();

    // Update status
    let intent = &mut ctx.accounts.intent;
    intent.status = IntentStatus::Expired;

    emit!(IntentExpired {
        intent_id: intent.intent_id,
        market_maker: intent.market_maker,
    });

    Ok(())
}

// ===== Flag Dispute =====

#[derive(Accounts)]
pub struct FlagDispute<'info> {
    pub signer: Signer<'info>,

    #[account(
        mut,
        constraint = intent.is_pending() @ ErrorCode::IntentNotPending,
        constraint = 
            signer.key() == intent.user || 
            signer.key() == intent.market_maker 
            @ ErrorCode::UnauthorizedDispute
    )]
    pub intent: Account<'info, Intent>,
}

pub fn handle_flag_dispute(
    ctx: Context<FlagDispute>,
    reason: String,
) -> Result<()> {
    require!(
        reason.len() <= MAX_DISPUTE_REASON_LEN,
        ErrorCode::DisputeReasonTooLong
    );

    let intent = &mut ctx.accounts.intent;
    intent.status = IntentStatus::Disputed;
    intent.disputed_by = Some(ctx.accounts.signer.key());
    intent.dispute_reason = Some(reason.clone());

    emit!(DisputeFlagged {
        intent_id: intent.intent_id,
        flagged_by: ctx.accounts.signer.key(),
        reason,
    });

    Ok(())
}

