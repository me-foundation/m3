use std::ops::Deref;

use anchor_lang::{prelude::*, AnchorDeserialize};
use mpl_bubblegum::accounts::TreeConfig;

#[account]
#[derive(Default, Copy)]
pub struct SellerTradeState {
    // Byte offsets:
    // 0
    // Discriminator

    // 8
    pub seller: Pubkey,
    // 40
    pub seller_referral: Pubkey,
    // 72
    pub buyer_price: u64,
    // 80
    pub asset_id: Pubkey,
    // 112
    pub payment_mint: Pubkey,
    // 144
    pub bump: u8,
    // 145
    pub merkle_tree: Pubkey, // Asset's Merkle Tree account.
    // 177
    pub index: u32, // Asset's index in the Merkle Tree.
    // 181
    pub created_at: i64,
    // 189
    pub updated_at: i64,
}

impl SellerTradeState {
    pub const LEN: usize = 8 + // discriminator
      32 + // seller
      32 + // seller_referral
      8 + // buyer_price
      32 + // asset_id
      32 + // payment_mint
      1 + // bump
      32 + // merkle_tree
      4 + // index
      8 + // created_at
      8 + // updated_at
      240; // padding
}

// Wrapper structs to replace the Anchor program types until the Metaplex libs have
// better Anchor support.
pub struct BubblegumProgram;

impl Id for BubblegumProgram {
    fn id() -> Pubkey {
        mpl_bubblegum::ID
    }
}

#[derive(Clone)]
pub struct TreeConfigAnchor(pub TreeConfig);

impl AccountDeserialize for TreeConfigAnchor {
    fn try_deserialize_unchecked(buf: &mut &[u8]) -> Result<Self> {
        Ok(Self(TreeConfig::from_bytes(buf)?))
    }
}

impl anchor_lang::Owner for TreeConfigAnchor {
    fn owner() -> Pubkey {
        // pub use spl_token::ID is used at the top of the file
        mpl_bubblegum::ID
    }
}

// No-op since we can't write data to a foreign program's account.
impl AccountSerialize for TreeConfigAnchor {}

impl Deref for TreeConfigAnchor {
    type Target = TreeConfig;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
