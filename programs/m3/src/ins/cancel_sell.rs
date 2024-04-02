use crate::{
    constants::*,
    errors::ErrorCode,
    states::{BubblegumProgram, SellerTradeState, TreeConfigAnchor},
    utils::transfer_compressed_nft,
};
use anchor_lang::{prelude::*, AnchorDeserialize};
use mpl_bubblegum::utils::get_asset_id;
use spl_account_compression::{program::SplAccountCompression, Noop};

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct CancelSellArgs {
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
    // This is the value of num_minted for the tree stored in the TreeConfig account at the time the NFT was minted.
    // The unique value for each asset can be retrieved from off-chain data store.
    nonce: u64,
    // The index of the leaf in the merkle tree. Can be retrieved from off-chain store.
    index: u32,
}

#[derive(Accounts)]
#[instruction(args:CancelSellArgs)]
pub struct CancelSell<'info> {
    // Listing owner
    #[account(mut)]
    /// CHECK: that this matches the seller in the seller_trade_state.
    wallet: UncheckedAccount<'info>,
    /// CHECK: program_as_signer
    #[account(
      seeds=[PREFIX.as_bytes(), SIGNER.as_bytes()],
      bump)]
    program_as_signer: UncheckedAccount<'info>, // escrow that currently owns the cNFT

    // ==== cNFT transfer args ==== //
    #[account(
      mut,
      seeds = [merkle_tree.key().as_ref()],
      seeds::program = bubblegum_program.key(),
      bump,
    )]
    /// CHECK: This account is neither written to nor read from.
    pub tree_authority: Account<'info, TreeConfigAnchor>,
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

    /// CHECK: Notary or cancel authority must sign. Explicit address checked in the handler.
    notary: UncheckedAccount<'info>,

    #[account(
      mut,
      close=wallet, // Close account after this instruction
      seeds=[
          PREFIX.as_bytes(),
          merkle_tree.key().as_ref(),
          args.index.to_le_bytes().as_ref(),
      ],
      bump=seller_trade_state.bump)]
    seller_trade_state: Box<Account<'info, SellerTradeState>>,
}

pub fn handle<'info>(
    ctx: Context<'_, '_, '_, 'info, CancelSell<'info>>,
    args: CancelSellArgs,
) -> Result<()> {
    let wallet = &ctx.accounts.wallet;
    let merkle_tree = &ctx.accounts.merkle_tree.clone();
    let seller_trade_state = &mut ctx.accounts.seller_trade_state;
    let notary = &ctx.accounts.notary;

    // Seller should match the seller of the seller-trade-state
    if seller_trade_state.seller != *wallet.key {
        return Err(ErrorCode::IncorrectOwner.into());
    }

    // Cancel Authority must sign, or Notary and seller must sign.
    let cancel_authority_signed = notary.is_signer && (*notary.key == CANCEL_AUTHORITY);
    let notary_signed = notary.is_signer && (*notary.key == ME_NOTARY);
    let valid_cancel = cancel_authority_signed || (wallet.is_signer && notary_signed);
    if !valid_cancel {
        return Err(ErrorCode::NoValidSignerPresent.into());
    }

    // Seller trade state should not be empty
    if seller_trade_state.to_account_info().data_is_empty() {
        return Err(ErrorCode::EmptyTradeState.into());
    }

    // AssetID should match the assetID of the seller-trade-state
    let asset_id = get_asset_id(&merkle_tree.key(), args.nonce);
    if asset_id != seller_trade_state.asset_id {
        return Err(ErrorCode::AssetIDMismatch.into());
    }

    let bump = ctx.bumps.program_as_signer;
    let seeds = &[PREFIX.as_bytes(), SIGNER.as_bytes(), &[bump][..]];
    transfer_compressed_nft(
        &ctx.accounts.tree_authority.to_account_info(),
        &ctx.accounts.program_as_signer.to_account_info(),
        &ctx.accounts.program_as_signer.to_account_info(), // delegate
        &wallet.to_account_info(),
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
        Some(seeds),
    )?;

    Ok(())
}
