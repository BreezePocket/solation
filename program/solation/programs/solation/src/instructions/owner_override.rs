use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};

use crate::constants::*;
use crate::errors::ErrorCode;
use crate::state::*;

// ===== Resolution Events =====

#[event]
pub struct DisputeResolved {
    pub intent_id: u64,
    pub resolution_type: String,
    pub resolved_by: Pubkey,
    pub reason: String,
}

#[event]
pub struct MutualUnwind {
    pub intent_id: u64,
    pub user: Pubkey,
    pub market_maker: Pubkey,
    pub user_returned: u64,
}

#[event]
pub struct ForceContinue {
    pub intent_id: u64,
    pub position_id: u64,
    pub reason: String,
}

#[event]
pub struct ForceSettleNow {
    pub intent_id: u64,
    pub settlement_price: u64,
    pub user_payout: u64,
    pub mm_payout: u64,
}

#[event]
pub struct EscrowToTreasury {
    pub intent_id: u64,
    pub amount: u64,
    pub reason: String,
}

#[event]
pub struct EmergencyShutdown {
    pub triggered_by: Pubkey,
    pub reason: String,
    pub timestamp: i64,
}

// ===== 1. MUTUAL UNWIND =====
// Both user and MM get their deposits back, no position created

#[derive(Accounts)]
pub struct MutualUnwindIntent<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        seeds = [GLOBAL_STATE_SEED],
        bump = global_state.bump,
        constraint = global_state.authority == authority.key() @ ErrorCode::Unauthorized
    )]
    pub global_state: Account<'info, GlobalState>,

    #[account(
        mut,
        constraint = intent.can_be_resolved() @ ErrorCode::IntentNotResolvable
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
    #[account(
        mut,
        constraint = user_token_account.owner == intent.user
    )]
    pub user_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

pub fn handle_mutual_unwind(
    ctx: Context<MutualUnwindIntent>,
    reason: String,
) -> Result<()> {
    require!(
        reason.len() <= MAX_DISPUTE_REASON_LEN,
        ErrorCode::DisputeReasonTooLong
    );

    let intent = &ctx.accounts.intent;
    let escrow_amount = intent.escrow_amount;

    // Return user escrow to user
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
    intent.status = IntentStatus::ResolvedToUser; // Mutual unwind = back to user

    emit!(MutualUnwind {
        intent_id: intent.intent_id,
        user: intent.user,
        market_maker: intent.market_maker,
        user_returned: escrow_amount,
    });

    emit!(DisputeResolved {
        intent_id: intent.intent_id,
        resolution_type: "MUTUAL_UNWIND".to_string(),
        resolved_by: ctx.accounts.authority.key(),
        reason,
    });

    msg!("Mutual unwind complete. User escrow returned.");
    Ok(())
}

// ===== 2. FORCE CONTINUE =====
// Force create the position as if MM had filled normally

#[derive(Accounts)]
pub struct ForceContinueIntent<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        seeds = [GLOBAL_STATE_SEED],
        bump = global_state.bump,
        constraint = global_state.authority == authority.key() @ ErrorCode::Unauthorized
    )]
    pub global_state: Account<'info, GlobalState>,

    #[account(
        mut,
        constraint = intent.can_be_resolved() @ ErrorCode::IntentNotResolvable
    )]
    pub intent: Account<'info, Intent>,

    #[account(
        mut,
        seeds = [MM_REGISTRY_SEED, intent.market_maker.as_ref()],
        bump = mm_registry.bump
    )]
    pub mm_registry: Account<'info, MMRegistry>,

    /// User's escrow (kept as position collateral)
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

    /// MM's token account to pay premium from (authority pays on behalf)
    /// In force continue, we might skip premium or use treasury
    #[account(mut)]
    pub premium_source: Account<'info, TokenAccount>,

    /// Position to create
    #[account(
        init,
        payer = authority,
        space = Position::LEN,
        seeds = [POSITION_SEED, intent.user.as_ref(), &intent.intent_id.to_le_bytes()],
        bump
    )]
    pub position: Account<'info, Position>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn handle_force_continue(
    ctx: Context<ForceContinueIntent>,
    reason: String,
    pay_premium: bool,
) -> Result<()> {
    require!(
        reason.len() <= MAX_DISPUTE_REASON_LEN,
        ErrorCode::DisputeReasonTooLong
    );

    let clock = Clock::get()?;
    let intent = &ctx.accounts.intent;

    // Optionally pay premium to user
    if pay_premium {
        let total_premium = intent.calculate_total_premium();
        let cpi_accounts = Transfer {
            from: ctx.accounts.premium_source.to_account_info(),
            to: ctx.accounts.user_token_account.to_account_info(),
            authority: ctx.accounts.authority.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::transfer(cpi_ctx, total_premium)?;
    }

    // Create Position
    let position = &mut ctx.accounts.position;
    position.position_id = intent.intent_id;
    position.user = intent.user;
    position.market_maker = intent.market_maker;
    position.strategy = intent.strategy;
    position.asset_mint = intent.asset_mint;
    position.quote_mint = intent.quote_mint;
    position.strike_price = intent.strike_price;
    position.premium_paid = if pay_premium { intent.calculate_total_premium() } else { 0 };
    position.contract_size = intent.contract_size;
    position.created_at = clock.unix_timestamp;
    position.expiry_timestamp = intent.quote_expiry;
    position.settlement_price = None;
    position.status = PositionStatus::Active;
    position.user_vault = intent.user_escrow;
    position.mm_vault_locked = ctx.accounts.premium_source.key();
    position.bump = ctx.bumps.position;
    position.user_vault_bump = 0;
    position.mm_vault_bump = 0;

    // Update MM stats
    let mm_registry = &mut ctx.accounts.mm_registry;
    mm_registry.record_fill(intent.contract_size, clock.unix_timestamp);

    // Update intent
    let intent = &mut ctx.accounts.intent;
    intent.status = IntentStatus::Filled;

    emit!(ForceContinue {
        intent_id: intent.intent_id,
        position_id: position.position_id,
        reason: reason.clone(),
    });

    emit!(DisputeResolved {
        intent_id: intent.intent_id,
        resolution_type: "FORCE_CONTINUE".to_string(),
        resolved_by: ctx.accounts.authority.key(),
        reason,
    });

    msg!("Force continue complete. Position created.");
    Ok(())
}

// ===== 3. FORCE SETTLE NOW =====
// Settle position immediately at current/specified price

#[derive(Accounts)]
pub struct ForceSettleNowIntent<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        seeds = [GLOBAL_STATE_SEED],
        bump = global_state.bump,
        constraint = global_state.authority == authority.key() @ ErrorCode::Unauthorized
    )]
    pub global_state: Account<'info, GlobalState>,

    #[account(
        mut,
        constraint = intent.can_be_resolved() @ ErrorCode::IntentNotResolvable
    )]
    pub intent: Account<'info, Intent>,

    /// User's escrow
    #[account(
        mut,
        seeds = [USER_ESCROW_SEED, intent.key().as_ref()],
        bump
    )]
    pub user_escrow: Account<'info, TokenAccount>,

    /// User's token account
    #[account(
        mut,
        constraint = user_token_account.owner == intent.user
    )]
    pub user_token_account: Account<'info, TokenAccount>,

    /// MM's token account
    #[account(
        mut,
        constraint = mm_token_account.owner == intent.market_maker
    )]
    pub mm_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

pub fn handle_force_settle_now(
    ctx: Context<ForceSettleNowIntent>,
    settlement_price: u64,
    user_payout_bps: u16, // Basis points to user (0-10000)
    reason: String,
) -> Result<()> {
    require!(user_payout_bps <= 10000, ErrorCode::InvalidPercentage);
    require!(
        reason.len() <= MAX_DISPUTE_REASON_LEN,
        ErrorCode::DisputeReasonTooLong
    );

    let intent = &ctx.accounts.intent;
    let escrow_amount = intent.escrow_amount;

    // Calculate payouts
    let user_payout = (escrow_amount as u128 * user_payout_bps as u128 / 10000) as u64;
    let mm_payout = escrow_amount.saturating_sub(user_payout);

    let intent_key = intent.key();
    let seeds = &[
        USER_ESCROW_SEED,
        intent_key.as_ref(),
        &[ctx.bumps.user_escrow],
    ];
    let signer_seeds = &[&seeds[..]];

    // Pay user
    if user_payout > 0 {
        let cpi_accounts = Transfer {
            from: ctx.accounts.user_escrow.to_account_info(),
            to: ctx.accounts.user_token_account.to_account_info(),
            authority: ctx.accounts.intent.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer_seeds);
        token::transfer(cpi_ctx, user_payout)?;
    }

    // Pay MM
    if mm_payout > 0 {
        let cpi_accounts = Transfer {
            from: ctx.accounts.user_escrow.to_account_info(),
            to: ctx.accounts.mm_token_account.to_account_info(),
            authority: ctx.accounts.intent.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer_seeds);
        token::transfer(cpi_ctx, mm_payout)?;
    }

    // Update intent
    let intent = &mut ctx.accounts.intent;
    intent.status = IntentStatus::ResolvedSplit;

    emit!(ForceSettleNow {
        intent_id: intent.intent_id,
        settlement_price,
        user_payout,
        mm_payout,
    });

    emit!(DisputeResolved {
        intent_id: intent.intent_id,
        resolution_type: "FORCE_SETTLE_NOW".to_string(),
        resolved_by: ctx.accounts.authority.key(),
        reason,
    });

    msg!("Force settle complete. User: {}, MM: {}", user_payout, mm_payout);
    Ok(())
}

// ===== 4. ESCROW TO TREASURY =====
// Move funds to treasury for manual distribution

#[derive(Accounts)]
pub struct EscrowToTreasuryIntent<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        seeds = [GLOBAL_STATE_SEED],
        bump = global_state.bump,
        constraint = global_state.authority == authority.key() @ ErrorCode::Unauthorized
    )]
    pub global_state: Account<'info, GlobalState>,

    #[account(
        mut,
        constraint = intent.can_be_resolved() @ ErrorCode::IntentNotResolvable
    )]
    pub intent: Account<'info, Intent>,

    /// User's escrow
    #[account(
        mut,
        seeds = [USER_ESCROW_SEED, intent.key().as_ref()],
        bump
    )]
    pub user_escrow: Account<'info, TokenAccount>,

    /// Treasury token account
    #[account(
        mut,
        constraint = treasury_token_account.owner == global_state.treasury
    )]
    pub treasury_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

pub fn handle_escrow_to_treasury(
    ctx: Context<EscrowToTreasuryIntent>,
    reason: String,
) -> Result<()> {
    require!(
        reason.len() <= MAX_DISPUTE_REASON_LEN,
        ErrorCode::DisputeReasonTooLong
    );

    let intent = &ctx.accounts.intent;
    let escrow_amount = intent.escrow_amount;

    let intent_key = intent.key();
    let seeds = &[
        USER_ESCROW_SEED,
        intent_key.as_ref(),
        &[ctx.bumps.user_escrow],
    ];
    let signer_seeds = &[&seeds[..]];

    // Transfer to treasury
    let cpi_accounts = Transfer {
        from: ctx.accounts.user_escrow.to_account_info(),
        to: ctx.accounts.treasury_token_account.to_account_info(),
        authority: ctx.accounts.intent.to_account_info(),
    };
    let cpi_program = ctx.accounts.token_program.to_account_info();
    let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer_seeds);
    token::transfer(cpi_ctx, escrow_amount)?;

    // Update intent - use Disputed status to indicate pending manual resolution
    let intent = &mut ctx.accounts.intent;
    intent.status = IntentStatus::Disputed; // Remains disputed until manual distribution

    emit!(EscrowToTreasury {
        intent_id: intent.intent_id,
        amount: escrow_amount,
        reason: reason.clone(),
    });

    emit!(DisputeResolved {
        intent_id: intent.intent_id,
        resolution_type: "ESCROW_TO_TREASURY".to_string(),
        resolved_by: ctx.accounts.authority.key(),
        reason,
    });

    msg!("Escrow moved to treasury for manual distribution.");
    Ok(())
}

// ===== 5. PROPORTIONAL SPLIT =====
// Split funds between user and MM by percentage

#[derive(Accounts)]
pub struct ProportionalSplitIntent<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        seeds = [GLOBAL_STATE_SEED],
        bump = global_state.bump,
        constraint = global_state.authority == authority.key() @ ErrorCode::Unauthorized
    )]
    pub global_state: Account<'info, GlobalState>,

    #[account(
        mut,
        constraint = intent.can_be_resolved() @ ErrorCode::IntentNotResolvable
    )]
    pub intent: Account<'info, Intent>,

    /// User's escrow
    #[account(
        mut,
        seeds = [USER_ESCROW_SEED, intent.key().as_ref()],
        bump
    )]
    pub user_escrow: Account<'info, TokenAccount>,

    /// User's token account
    #[account(
        mut,
        constraint = user_token_account.owner == intent.user
    )]
    pub user_token_account: Account<'info, TokenAccount>,

    /// MM's token account
    #[account(
        mut,
        constraint = mm_token_account.owner == intent.market_maker
    )]
    pub mm_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

pub fn handle_proportional_split(
    ctx: Context<ProportionalSplitIntent>,
    user_bps: u16,
    reason: String,
) -> Result<()> {
    require!(user_bps <= 10000, ErrorCode::InvalidPercentage);
    require!(
        reason.len() <= MAX_DISPUTE_REASON_LEN,
        ErrorCode::DisputeReasonTooLong
    );

    let intent = &ctx.accounts.intent;
    let escrow_amount = intent.escrow_amount;

    let user_amount = (escrow_amount as u128 * user_bps as u128 / 10000) as u64;
    let mm_amount = escrow_amount.saturating_sub(user_amount);

    let intent_key = intent.key();
    let seeds = &[
        USER_ESCROW_SEED,
        intent_key.as_ref(),
        &[ctx.bumps.user_escrow],
    ];
    let signer_seeds = &[&seeds[..]];

    // Transfer user portion
    if user_amount > 0 {
        let cpi_accounts = Transfer {
            from: ctx.accounts.user_escrow.to_account_info(),
            to: ctx.accounts.user_token_account.to_account_info(),
            authority: ctx.accounts.intent.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer_seeds);
        token::transfer(cpi_ctx, user_amount)?;
    }

    // Transfer MM portion
    if mm_amount > 0 {
        let cpi_accounts = Transfer {
            from: ctx.accounts.user_escrow.to_account_info(),
            to: ctx.accounts.mm_token_account.to_account_info(),
            authority: ctx.accounts.intent.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer_seeds);
        token::transfer(cpi_ctx, mm_amount)?;
    }

    let intent = &mut ctx.accounts.intent;
    intent.status = IntentStatus::ResolvedSplit;

    emit!(DisputeResolved {
        intent_id: intent.intent_id,
        resolution_type: format!("PROPORTIONAL_SPLIT_{}bps", user_bps),
        resolved_by: ctx.accounts.authority.key(),
        reason,
    });

    msg!("Proportional split complete. User: {} ({}bps), MM: {}", 
         user_amount, user_bps, mm_amount);
    Ok(())
}

// ===== 6. EMERGENCY SHUTDOWN =====
// Global pause and unwind all pending intents

#[derive(Accounts)]
pub struct TriggerEmergencyShutdown<'info> {
    #[account(mut)]
    pub authority: Signer<'info>,

    #[account(
        mut,
        seeds = [GLOBAL_STATE_SEED],
        bump = global_state.bump,
        constraint = global_state.authority == authority.key() @ ErrorCode::Unauthorized
    )]
    pub global_state: Account<'info, GlobalState>,
}

pub fn handle_emergency_shutdown(
    ctx: Context<TriggerEmergencyShutdown>,
    reason: String,
) -> Result<()> {
    let clock = Clock::get()?;
    
    // Pause the protocol
    let global_state = &mut ctx.accounts.global_state;
    global_state.paused = true;

    emit!(EmergencyShutdown {
        triggered_by: ctx.accounts.authority.key(),
        reason: reason.clone(),
        timestamp: clock.unix_timestamp,
    });

    msg!("EMERGENCY SHUTDOWN triggered. Protocol paused. Reason: {}", reason);
    msg!("All pending intents should be unwound manually via mutual_unwind.");
    
    Ok(())
}
