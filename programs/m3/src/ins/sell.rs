use crate::{
    constants::*,
    errors::ErrorCode,
    states::{BubblegumProgram, SellerTradeState, TreeConfigAnchor},
    utils::transfer_compressed_nft,
};
use anchor_lang::{prelude::*, AnchorDeserialize, Discriminator};
use mpl_bubblegum::utils::get_asset_id;
use spl_account_compression::{program::SplAccountCompression, Noop};

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct SellArgs {
    // === cNFT transfer args === //
    // The Merkle root for the tree. Can be retrieved from off-chain data store.
    root: [u8; 32],
    // The Keccak256 hash of the NFTs existing metadata (without the verified flag for the creator changed).
    // The metadata is retrieved from off-chain data store.
    data_hash: [u8; 32],
    // The Keccak256 hash of the NFTs existing creators array (without the verified flag for the creator changed).
    // The creators array is retrieved from off-chain data store.
    creator_hash: [u8; 32],
    // A nonce ("number used once") value used to make the Merkle tree leaves unique.
    //This is the value of num_minted for the tree stored in the TreeConfig account at the time the NFT was minted.
    // The unique value for each asset can be retrieved from off-chain data store.
    nonce: u64,
    // The index of the leaf in the merkle tree. Can be retrieved from off-chain store.
    index: u32,
    // === Contract args === //
    // Price of the NFT in the payment_mint.
    buyer_price: u64,
    // The mint of the SPL token used to pay for the NFT.
    payment_mint: Pubkey,
}

#[derive(Accounts)]
#[instruction(args:SellArgs)]
pub struct Sell<'info> {
    // Listing owner
    #[account(mut)]
    wallet: Signer<'info>,
    /// CHECK: program_as_signer
    #[account(
      seeds=[PREFIX.as_bytes(), SIGNER.as_bytes()],
      bump)]
    program_as_signer: UncheckedAccount<'info>, // escrow to hold the ownership of the cnft

    // ==== cNFT transfer args ==== //
    #[account(
      mut,
      seeds = [merkle_tree.key().as_ref()],
      seeds::program = bubblegum_program.key(),
      bump,
    )]
    /// CHECK: This account is neither written to nor read from.
    pub tree_authority: Account<'info, TreeConfigAnchor>,
    // The NFT delegate. Transfers must be signed by either the NFT owner or NFT delegate.
    /// CHECK: This account is checked in the Bubblegum transfer instruction
    leaf_delegate: UncheckedAccount<'info>,
    // The account that contains the Merkle tree, initialized by create_tree.
    /// CHECK: This account is modified in the downstream Bubblegum program
    #[account(mut)]
    merkle_tree: UncheckedAccount<'info>,
    // Used by bubblegum for logging (CPI)
    log_wrapper: Program<'info, Noop>,

    bubblegum_program: Program<'info, BubblegumProgram>,

    system_program: Program<'info, System>,

    // The Solana Program Library spl-account-compression program ID.
    compression_program: Program<'info, SplAccountCompression>,

    /// CHECK: seller_referral - not sure we need to check this
    seller_referral: UncheckedAccount<'info>,

    #[account(
      init_if_needed,
      payer=wallet,
      seeds=[
          PREFIX.as_bytes(),
          merkle_tree.key().as_ref(),
          args.index.to_le_bytes().as_ref(),
      ],
      space=SellerTradeState::LEN,
      bump)]
    seller_trade_state: Box<Account<'info, SellerTradeState>>,
}

pub fn handle<'info>(ctx: Context<'_, '_, '_, 'info, Sell<'info>>, args: SellArgs) -> Result<()> {
    let wallet = &ctx.accounts.wallet.clone();
    let merkle_tree = &ctx.accounts.merkle_tree.clone();
    let seller_trade_state_clone = &ctx.accounts.seller_trade_state.to_account_info();
    let seller_trade_state = &mut ctx.accounts.seller_trade_state;
    let discriminator_ai = seller_trade_state_clone.try_borrow_data()?;
    let seller_referral = &ctx.accounts.seller_referral.clone();
    let bump = ctx.bumps.seller_trade_state;
    // Validate discriminator
    if discriminator_ai[..8] != SellerTradeState::discriminator() && discriminator_ai[..8] != [0; 8]
    {
        return Err(ErrorCode::InvalidDiscriminator.into());
    }
    // Validate price.
    if args.buyer_price > MAX_PRICE || args.buyer_price == 0 {
        return Err(ErrorCode::InvalidPrice.into());
    }

    let asset_id = get_asset_id(&merkle_tree.key(), args.nonce);
    let is_new_listing = seller_trade_state.asset_id == Pubkey::default();

    // Transfer the NFT to M3 Program if seller_Trade_state was just instantiated
    if is_new_listing {
        msg!(
            "Transferring asset to: {}",
            ctx.accounts.program_as_signer.key
        );
        transfer_compressed_nft(
            &ctx.accounts.tree_authority.to_account_info(),
            &wallet.to_account_info(),
            &ctx.accounts.leaf_delegate.to_account_info(), // delegate
            &ctx.accounts.program_as_signer.to_account_info(),
            &ctx.accounts.merkle_tree,
            &ctx.accounts.log_wrapper,
            &ctx.accounts.compression_program,
            &ctx.accounts.system_program,
            ctx.remaining_accounts,
            ctx.accounts.bubblegum_program.key(),
            args.root,
            args.data_hash,
            args.creator_hash,
            args.nonce,
            args.index,
            None, // signer passed through from ctx
        )?;
        seller_trade_state.created_at = Clock::get()?.unix_timestamp;
    } else {
        if seller_trade_state.seller != wallet.key() {
            msg!(
                "Seller mismatch when re-pricing listing: {} != {}",
                seller_trade_state.seller,
                wallet.key()
            );
            return Err(ErrorCode::SellerMismatch.into());
        }
        if seller_trade_state.asset_id != asset_id
            || seller_trade_state.merkle_tree != merkle_tree.key()
        {
            msg!(
                "Asset ID mismatch when re-pricing listing: {} != {}",
                seller_trade_state.asset_id,
                asset_id
            );
            return Err(ErrorCode::AssetIDMismatch.into());
        }
        msg!("Updating price to: {}", args.buyer_price);
    }

    seller_trade_state.seller = wallet.key();
    seller_trade_state.seller_referral = seller_referral.key();
    seller_trade_state.buyer_price = args.buyer_price;
    seller_trade_state.asset_id = asset_id;
    seller_trade_state.bump = bump;
    seller_trade_state.merkle_tree = ctx.accounts.merkle_tree.key();
    seller_trade_state.index = args.index;
    seller_trade_state.updated_at = Clock::get()?.unix_timestamp;

    Ok(())
}
