use anchor_lang::prelude::*;
use solana_program::{program::invoke, system_instruction};

use crate::{
    constants::*,
    states::{BubblegumProgram, SellerTradeState, TreeConfigAnchor},
    utils::{
        calculate_fees, hash_metadata_data, pay_creator_fees, transfer_compressed_nft,
        verify_creators,
    },
};
use anchor_lang::AnchorDeserialize;
use spl_account_compression::{program::SplAccountCompression, Noop};

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct BuyNowArgs {
    // Execute Sale Args
    buyer_price: u64,
    maker_fee_bp: i16,
    taker_fee_bp: u16,
    // This argument is ignored, 100% royalties are enforced by the contract
    // We leave the argument here in case we want to change this in the future.
    buyer_creator_royalty_bp: u16,
    // Following arguments are required for paying creator royalties
    creator_shares: Vec<u16>,
    creator_verified: Vec<bool>,
    // Creator royalties. Validated against the metadata_hash by Bubblegum after hashing with metadata_hash.
    seller_fee_basis_points: u16,

    // === cNFT transfer args === //
    // The Merkle root for the tree. Can be retrieved from off-chain data store.
    root: [u8; 32],
    // The Keccak256 hash of the NFTs existing metadata (without the verified flag for the creator changed).
    // Does not include the extra seller-fee-basis-points hash that's required by Bubblegum.
    // The metadata is retrieved from off-chain data store.
    metadata_hash: [u8; 32],
    // The Keccak256 hash of the NFTs existing creators array (without the verified flag for the creator changed).
    // The creators array is retrieved from off-chain data store.
    creator_hash: [u8; 32],
    // A nonce ("number used once") value used to make the Merkle tree leaves unique.
    // This is the value of num_minted for the tree stored in the TreeConfig account at the time the NFT was minted.
    // The unique value for each asset can be retrxieved from off-chain data store.
    nonce: u64,
    // The index of the leaf in the merkle tree. Can be retrieved from off-chain store.
    index: u32,
}

#[derive(Accounts)]
#[instruction(args:BuyNowArgs)]
pub struct BuyNow<'info> {
    #[account(mut)]
    buyer: Signer<'info>,
    /// CHECK: seller checked in seller_trade_state.
    #[account(mut)]
    seller: UncheckedAccount<'info>,
    /// CHECK: meNotary constant
    #[account(address = ME_NOTARY)]
    notary: UncheckedAccount<'info>,
    /// CHECK: that this matches hard-coded auction_house_treasury
    #[account(mut, address = ME_TREASURY)]
    platform_treasury: UncheckedAccount<'info>,

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

    #[account(mut,
      close=seller,
      constraint= seller_trade_state.seller_referral == seller_referral.key(),
      constraint= seller_trade_state.buyer_price == args.buyer_price,
      constraint= seller_trade_state.seller == seller.key(),
      seeds=[
          PREFIX.as_bytes(),
          merkle_tree.key().as_ref(),
          args.index.to_le_bytes().as_ref(),
      ], bump)]
    seller_trade_state: Box<Account<'info, SellerTradeState>>,
    /// CHECK: program_as_signer
    #[account(seeds=[PREFIX.as_bytes(), SIGNER.as_bytes()], bump)]
    program_as_signer: UncheckedAccount<'info>,

    /// CHECK: seller_referral
    seller_referral: UncheckedAccount<'info>,

    /// CHECK: only receives the asset.
    receiver: UncheckedAccount<'info>,
}

pub fn handle<'info>(
    ctx: Context<'_, '_, '_, 'info, BuyNow<'info>>,
    args: BuyNowArgs,
) -> Result<()> {
    let buyer = &ctx.accounts.buyer.to_account_info();
    let seller = &ctx.accounts.seller.to_account_info();
    let notary = &ctx.accounts.notary;
    let platform_treasury = &ctx.accounts.platform_treasury;
    let _seller_trade_state = &mut ctx.accounts.seller_trade_state;
    let system_program = &ctx.accounts.system_program;
    let _program_as_signer = &ctx.accounts.program_as_signer;

    // Remaining accounts are 1. (Optional) creator addresses and 2. Merkle proof path.
    let creator_shares_length = args.creator_shares.len();
    let creator_shares_clone = args.creator_shares.clone();
    let (creator_accounts, proof_path) = ctx.remaining_accounts.split_at(creator_shares_length);

    // Create data_hash from metadata_hash + seller_fee_basis_points (secures creator royalties)
    let data_hash = hash_metadata_data(args.metadata_hash, args.seller_fee_basis_points)?;

    // 1: Pay Creator Fees
    // Bubblegum will verify the creator_hash for us, but we should verify the input creators match.
    verify_creators(
        creator_accounts.iter(),
        args.creator_shares,
        args.creator_verified,
        args.creator_hash,
    )?;
    pay_creator_fees(
        &mut creator_accounts.iter(),
        creator_shares_clone,
        &buyer.to_account_info(),
        system_program,
        args.buyer_price,
        DEFAULT_CREATOR_ROYALTY_BP,
        args.seller_fee_basis_points,
    )?;

    // 2: Calculate fees
    let fees = calculate_fees(
        notary,
        args.buyer_price,
        args.maker_fee_bp,
        args.taker_fee_bp,
        buyer,
        seller,
    )?;

    // 3: Buyer pays Seller
    invoke(
        &system_instruction::transfer(&buyer.key(), seller.key, fees.seller_will_get_from_buyer),
        &[
            buyer.to_account_info(),
            seller.to_account_info(),
            system_program.to_account_info(),
        ],
    )?;

    // 4. Buyer pays Treasury
    let treasury_clone = platform_treasury.to_account_info();
    if fees.total_platform_fee > 0 {
        invoke(
            &system_instruction::transfer(
                &buyer.key(),
                treasury_clone.key,
                fees.total_platform_fee,
            ),
            &[
                buyer.to_account_info(),
                treasury_clone.to_account_info(),
                system_program.to_account_info(),
            ],
        )?;
    }

    // 5. Transfer NFT to Buyer
    let bump = ctx.bumps.program_as_signer;
    let seeds = &[PREFIX.as_bytes(), SIGNER.as_bytes(), &[bump][..]];
    transfer_compressed_nft(
        &ctx.accounts.tree_authority.to_account_info(),
        // Transfer the NFT from the M3 escrow to the buyer.
        &ctx.accounts.program_as_signer.to_account_info(),
        &ctx.accounts.program_as_signer.to_account_info(), // delegate
        &ctx.accounts.receiver.to_account_info(),
        &ctx.accounts.merkle_tree,
        &ctx.accounts.log_wrapper,
        &ctx.accounts.compression_program,
        &ctx.accounts.system_program,
        proof_path,
        ctx.accounts.bubblegum_program.key(),
        args.root,
        data_hash,
        args.creator_hash, // This is secured by Bubblegum (important for paying creators)
        args.nonce,
        args.index,
        Some(seeds),
    )?;

    msg!(
        "{{\"price\":{},\"maker_fee\":{},\"taker_fee\":{},\"total_platform_fee\":{}}}",
        args.buyer_price,
        fees.maker_fee,
        fees.taker_fee,
        fees.total_platform_fee
    );

    Ok(())
}
