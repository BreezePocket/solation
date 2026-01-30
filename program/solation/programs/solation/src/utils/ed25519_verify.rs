use anchor_lang::prelude::*;
use anchor_lang::solana_program::sysvar::instructions::{
    load_instruction_at_checked, ID as INSTRUCTIONS_SYSVAR_ID,
};
use anchor_lang::solana_program::pubkey;

use crate::errors::ErrorCode;
use crate::state::StrategyType;

/// Ed25519 program ID
pub const ED25519_PROGRAM_ID: Pubkey = pubkey!("Ed25519SigVerify111111111111111111111111111");

/// Ed25519 signature offsets struct (matches Solana's expected format)
#[derive(Clone, Copy)]
#[repr(C)]
pub struct Ed25519SignatureOffsets {
    pub signature_offset: u16,
    pub signature_instruction_index: u16,
    pub public_key_offset: u16,
    pub public_key_instruction_index: u16,
    pub message_data_offset: u16,
    pub message_data_size: u16,
    pub message_instruction_index: u16,
}

/// Construct the quote message that MM should sign
/// Format: asset_mint || quote_mint || strategy || strike || premium || size || expiry || nonce
pub fn construct_quote_message(
    asset_mint: &Pubkey,
    quote_mint: &Pubkey,
    strategy: StrategyType,
    strike_price: u64,
    premium_per_contract: u64,
    contract_size: u64,
    quote_expiry: i64,
    quote_nonce: u64,
) -> Vec<u8> {
    let mut message = Vec::with_capacity(32 + 32 + 1 + 8 + 8 + 8 + 8 + 8);
    message.extend_from_slice(&asset_mint.to_bytes());
    message.extend_from_slice(&quote_mint.to_bytes());
    message.push(strategy as u8);
    message.extend_from_slice(&strike_price.to_le_bytes());
    message.extend_from_slice(&premium_per_contract.to_le_bytes());
    message.extend_from_slice(&contract_size.to_le_bytes());
    message.extend_from_slice(&quote_expiry.to_le_bytes());
    message.extend_from_slice(&quote_nonce.to_le_bytes());
    message
}

/// Verify Ed25519 signature by introspecting the transaction's Ed25519Program instruction.
/// 
/// The caller must include an Ed25519Program instruction BEFORE calling this instruction.
/// This function verifies that:
/// 1. An Ed25519Program instruction exists at the expected index
/// 2. The public key in that instruction matches the expected MM signing key
/// 3. The message in that instruction matches our expected quote message
/// 
/// # Arguments
/// * `instructions_sysvar` - The Instructions sysvar account
/// * `expected_signing_key` - The MM's registered signing key
/// * `expected_message` - The constructed quote message to verify
/// * `ed25519_instruction_index` - Index of the Ed25519Program instruction in the transaction
pub fn verify_ed25519_signature(
    instructions_sysvar: &AccountInfo,
    expected_signing_key: &Pubkey,
    expected_message: &[u8],
    ed25519_instruction_index: u8,
) -> Result<()> {
    // Verify we have the correct sysvar
    require!(
        instructions_sysvar.key == &INSTRUCTIONS_SYSVAR_ID,
        ErrorCode::InvalidSignature
    );

    // Load the Ed25519Program instruction
    let ed25519_ix = load_instruction_at_checked(
        ed25519_instruction_index as usize,
        instructions_sysvar,
    ).map_err(|_| ErrorCode::InvalidSignature)?;

    // Verify it's the Ed25519 program
    require!(
        ed25519_ix.program_id == ED25519_PROGRAM_ID,
        ErrorCode::InvalidSignature
    );

    // The Ed25519Program instruction data format:
    // [0]: num_signatures (u8)
    // [1]: padding (u8) 
    // [2..]: Ed25519SignatureOffsets for each signature
    // Then: signature data, pubkey data, message data
    
    let data = &ed25519_ix.data;
    
    // Need at least 2 bytes for header
    require!(data.len() >= 2, ErrorCode::InvalidSignature);
    
    let num_signatures = data[0];
    require!(num_signatures == 1, ErrorCode::InvalidSignature);

    // Parse the signature offsets (14 bytes)
    require!(data.len() >= 16, ErrorCode::InvalidSignature); // 2 header + 14 offsets
    
    let offsets = Ed25519SignatureOffsets {
        signature_offset: u16::from_le_bytes([data[2], data[3]]),
        signature_instruction_index: u16::from_le_bytes([data[4], data[5]]),
        public_key_offset: u16::from_le_bytes([data[6], data[7]]),
        public_key_instruction_index: u16::from_le_bytes([data[8], data[9]]),
        message_data_offset: u16::from_le_bytes([data[10], data[11]]),
        message_data_size: u16::from_le_bytes([data[12], data[13]]),
        message_instruction_index: u16::from_le_bytes([data[14], data[15]]),
    };

    // Extract the public key from the instruction data
    let pubkey_start = offsets.public_key_offset as usize;
    let pubkey_end = pubkey_start + 32;
    require!(data.len() >= pubkey_end, ErrorCode::InvalidSignature);
    
    let pubkey_bytes: [u8; 32] = data[pubkey_start..pubkey_end]
        .try_into()
        .map_err(|_| ErrorCode::InvalidSignature)?;
    let pubkey = Pubkey::new_from_array(pubkey_bytes);
    
    // Verify the public key matches the expected signing key
    require!(
        pubkey == *expected_signing_key,
        ErrorCode::SigningKeyMismatch
    );

    // Extract the message from the instruction data
    let msg_start = offsets.message_data_offset as usize;
    let msg_end = msg_start + offsets.message_data_size as usize;
    require!(data.len() >= msg_end, ErrorCode::InvalidSignature);
    
    let message = &data[msg_start..msg_end];
    
    // Verify the message matches our expected quote message
    require!(
        message == expected_message,
        ErrorCode::InvalidSignature
    );

    // If we get here, the Ed25519 program verified the signature
    // and we've confirmed the pubkey and message match our expectations
    msg!("Ed25519 signature verified successfully");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_construct_quote_message() {
        let asset_mint = Pubkey::new_unique();
        let quote_mint = Pubkey::new_unique();
        let strategy = StrategyType::CoveredCall;
        let strike_price = 50000_000000u64; // $50,000
        let premium = 1000_000000u64; // $1,000
        let size = 1_000000u64; // 1 contract
        let expiry = 1700000000i64;
        let nonce = 12345u64;

        let msg = construct_quote_message(
            &asset_mint,
            &quote_mint,
            strategy,
            strike_price,
            premium,
            size,
            expiry,
            nonce,
        );

        // 32 + 32 + 1 + 8 + 8 + 8 + 8 + 8 = 105 bytes
        assert_eq!(msg.len(), 105);
        
        // Verify asset_mint is first
        assert_eq!(&msg[0..32], &asset_mint.to_bytes());
        // Verify quote_mint is second
        assert_eq!(&msg[32..64], &quote_mint.to_bytes());
        // Verify strategy
        assert_eq!(msg[64], 0); // CoveredCall = 0
    }
}
