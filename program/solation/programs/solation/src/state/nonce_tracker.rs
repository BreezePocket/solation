use anchor_lang::prelude::*;

/// Nonce tracker for preventing replay attacks on quotes
/// Uses a bitmap to efficiently track used nonces
#[account]
pub struct NonceTracker {
    /// Market maker this tracker belongs to
    pub market_maker: Pubkey,
    /// Base nonce value (nonces are relative to this)
    pub base_nonce: u64,
    /// Bitmap of used nonces (256 bits = 32 bytes)
    /// Each bit represents whether base_nonce + bit_position has been used
    pub used_bitmap: [u8; 32],
    /// PDA bump
    pub bump: u8,
}

impl NonceTracker {
    pub const BITMAP_SIZE: usize = 256; // 32 bytes * 8 bits

    pub const LEN: usize = 8 +   // discriminator
        32 +  // market_maker
        8 +   // base_nonce
        32 +  // used_bitmap
        1;    // bump

    /// Check if a nonce has been used
    pub fn is_used(&self, nonce: u64) -> bool {
        if nonce < self.base_nonce {
            // Nonce is before our tracking window - assume used
            return true;
        }
        
        let offset = nonce - self.base_nonce;
        if offset >= Self::BITMAP_SIZE as u64 {
            // Nonce is beyond our window - not tracked yet
            return false;
        }

        let byte_index = (offset / 8) as usize;
        let bit_index = (offset % 8) as u8;
        
        (self.used_bitmap[byte_index] & (1 << bit_index)) != 0
    }

    /// Mark a nonce as used
    pub fn mark_used(&mut self, nonce: u64) -> Result<()> {
        if nonce < self.base_nonce {
            // Already in used range
            return Ok(());
        }

        let offset = nonce - self.base_nonce;
        
        // If nonce is beyond our window, we need to shift the window
        if offset >= Self::BITMAP_SIZE as u64 {
            let shift = offset - Self::BITMAP_SIZE as u64 + 1;
            self.shift_window(shift);
            return self.mark_used(nonce); // Recurse with updated window
        }

        let byte_index = (offset / 8) as usize;
        let bit_index = (offset % 8) as u8;
        
        self.used_bitmap[byte_index] |= 1 << bit_index;
        
        Ok(())
    }

    /// Shift the tracking window forward
    fn shift_window(&mut self, shift: u64) {
        if shift >= Self::BITMAP_SIZE as u64 {
            // Complete reset
            self.base_nonce = self.base_nonce.saturating_add(shift);
            self.used_bitmap = [0; 32];
            return;
        }

        let shift_bytes = (shift / 8) as usize;
        let shift_bits = (shift % 8) as u8;

        // Shift bytes
        if shift_bytes > 0 {
            for i in 0..(32 - shift_bytes) {
                self.used_bitmap[i] = self.used_bitmap[i + shift_bytes];
            }
            for i in (32 - shift_bytes)..32 {
                self.used_bitmap[i] = 0;
            }
        }

        // Shift remaining bits
        if shift_bits > 0 {
            let mut carry = 0u8;
            for i in (0..32).rev() {
                let new_carry = self.used_bitmap[i] >> (8 - shift_bits);
                self.used_bitmap[i] = (self.used_bitmap[i] << shift_bits) | carry;
                carry = new_carry;
            }
        }

        self.base_nonce = self.base_nonce.saturating_add(shift);
    }
}
