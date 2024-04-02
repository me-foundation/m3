use mpl_bubblegum::{hash::hash_creators, types::Creator};
use solana_program::keccak;
use spl_account_compression::{program::SplAccountCompression, Noop};

use crate::{
    constants::{DEFAULT_MAKER_FEE_BP, DEFAULT_TAKER_FEE_BP, MAX_FEE_ABS_BP},
    errors::ErrorCode,
};

use {
    anchor_lang::{
        prelude::*,
        solana_program::{program::invoke, program::invoke_signed, system_instruction},
    },
    arrayref::array_ref,
    std::{convert::TryFrom, convert::TryInto, slice::Iter},
};

/// Cheap method to just grab mint Pubkey from token account, instead of deserializing entire thing
#[allow(dead_code)]
pub fn get_mint_from_token_account(token_account_info: &AccountInfo) -> Result<Pubkey> {
    // TokeAccount layout:   mint(32), owner(32), ...
    let data = token_account_info.try_borrow_data()?;
    let mint_data = array_ref![data, 0, 32];
    Ok(Pubkey::new_from_array(*mint_data))
}

/// Create account almost from scratch, lifted from
/// https://github.com/solana-labs/solana-program-library/blob/7d4873c61721aca25464d42cc5ef651a7923ca79/associated-token-account/program/src/processor.rs#L51-L98
#[inline(always)]
#[allow(dead_code)]
pub fn create_or_allocate_account_raw<'a>(
    program_id: Pubkey,
    new_account_info: &AccountInfo<'a>,
    rent_sysvar_info: &AccountInfo<'a>,
    system_program_info: &AccountInfo<'a>,
    payer_info: &AccountInfo<'a>,
    size: usize,
    new_acct_seeds: &[&[u8]],
) -> Result<()> {
    let rent = &Rent::from_account_info(rent_sysvar_info)?;
    let required_lamports = rent
        .minimum_balance(size)
        .max(1)
        .saturating_sub(new_account_info.lamports());

    if required_lamports > 0 {
        invoke(
            &system_instruction::transfer(payer_info.key, new_account_info.key, required_lamports),
            &[
                payer_info.clone(),
                new_account_info.clone(),
                system_program_info.clone(),
            ],
        )?;
    }

    let accounts = &[new_account_info.clone(), system_program_info.clone()];
    invoke_signed(
        &system_instruction::allocate(new_account_info.key, size.try_into().unwrap()),
        accounts,
        &[new_acct_seeds],
    )?;

    invoke_signed(
        &system_instruction::assign(new_account_info.key, &program_id),
        accounts,
        &[new_acct_seeds],
    )?;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn transfer_compressed_nft<'info>(
    tree_authority: &AccountInfo<'info>,
    leaf_owner: &AccountInfo<'info>,
    leaf_delegate: &AccountInfo<'info>,
    new_leaf_owner: &AccountInfo<'info>,
    merkle_tree: &AccountInfo<'info>,
    log_wrapper: &Program<'info, Noop>,
    compression_program: &Program<'info, SplAccountCompression>,
    system_program: &Program<'info, System>,
    proof_path: &[AccountInfo<'info>],
    bubblegum_program_key: Pubkey,
    root: [u8; 32],
    data_hash: [u8; 32],
    creator_hash: [u8; 32],
    nonce: u64,
    index: u32,
    signer_seeds: Option<&[&[u8]]>,
) -> Result<()> {
    // proof_path are the accounts that make up the required proof
    let proof_path_len = proof_path.len();
    let mut accounts = Vec::with_capacity(
        8 // space for the 8 AccountMetas that are always included  (below)
    + proof_path_len,
    );
    accounts.extend(vec![
        AccountMeta::new_readonly(tree_authority.key(), false),
        AccountMeta::new_readonly(leaf_owner.key(), true),
        AccountMeta::new_readonly(leaf_delegate.key(), false),
        AccountMeta::new_readonly(new_leaf_owner.key(), false),
        AccountMeta::new(merkle_tree.key(), false),
        AccountMeta::new_readonly(log_wrapper.key(), false),
        AccountMeta::new_readonly(compression_program.key(), false),
        AccountMeta::new_readonly(system_program.key(), false),
    ]);

    let transfer_discriminator: [u8; 8] = [163, 52, 200, 231, 140, 3, 69, 186];

    let mut data = Vec::with_capacity(
        8 // The length of transfer_discriminator,
    + root.len()
    + data_hash.len()
    + creator_hash.len()
    + 8 // The length of the nonce
    + 8, // The length of the index
    );
    data.extend(transfer_discriminator);
    data.extend(root);
    data.extend(data_hash);
    data.extend(creator_hash);
    data.extend(nonce.to_le_bytes());
    data.extend(index.to_le_bytes());

    let mut account_infos = Vec::with_capacity(
        8 // space for the 8 AccountInfos that are always included (below)
    + proof_path_len,
    );
    account_infos.extend(vec![
        tree_authority.to_account_info(),
        leaf_owner.to_account_info(),
        leaf_delegate.to_account_info(),
        new_leaf_owner.to_account_info(),
        merkle_tree.to_account_info(),
        log_wrapper.to_account_info(),
        compression_program.to_account_info(),
        system_program.to_account_info(),
    ]);

    // Add "accounts" (hashes) that make up the merkle proof from the remaining accounts.
    for acc in proof_path.iter() {
        accounts.push(AccountMeta::new_readonly(acc.key(), false));
        account_infos.push(acc.to_account_info());
    }

    let instruction = solana_program::instruction::Instruction {
        program_id: bubblegum_program_key,
        accounts,
        data,
    };

    match signer_seeds {
        Some(seeds) => {
            let seeds_array: &[&[&[u8]]] = &[seeds];
            solana_program::program::invoke_signed(&instruction, &account_infos[..], seeds_array)
        }
        None => solana_program::program::invoke(&instruction, &account_infos[..]),
    }?;
    Ok(())
}

fn multiply_divide(value: u128, multiplier: u128, divisor: u128) -> Result<u128> {
    value
        .checked_mul(multiplier)
        .and_then(|result| result.checked_div(divisor))
        .ok_or(ErrorCode::NumericalOverflow.into())
}

#[allow(clippy::too_many_arguments)]
pub fn pay_creator_fees<'a>(
    creator_accounts: &mut Iter<AccountInfo<'a>>,
    creator_shares: Vec<u16>,
    escrow_payment_account: &AccountInfo<'a>,
    system_program: &AccountInfo<'a>,
    total_price: u64,
    buyer_creator_royalty_bp: u16,
    seller_fee_basis_points: u16,
) -> Result<u64> {
    // If there are no creators, return early with 0 fee paid
    if creator_accounts.len() == 0 {
        return Ok(0);
    }
    // Check if the lengths of remaining_accounts, creator_shares are the same
    if creator_accounts.len() != creator_shares.len() {
        msg!(
            "Mismatched creator share lengths: {{\"creators\":{},\"shares\":{}}}",
            creator_accounts.len(),
            creator_shares.len(),
        );
        return Err(ErrorCode::MismatchedCreatorDataLengths.into());
    }

    let royalty_bp = seller_fee_basis_points;

    // Royalty and buyerCreatorRoyalty
    let total_fee = multiply_divide(total_price as u128, royalty_bp as u128, 10000)
        .and_then(|result| multiply_divide(result, buyer_creator_royalty_bp as u128, 10000))?
        as u64;

    let mut total_fee_paid = 0u64;
    let mut total_pct = 0u16; // Validate that shares add up to 1

    for (index, creator_account) in creator_accounts.enumerate() {
        let share = creator_shares[index];
        total_pct = total_pct
            .checked_add(share)
            .ok_or(ErrorCode::CreatorShareTotalMustBe100)?;

        let pct = share as u128;

        let creator_fee = pct
            .checked_mul(total_fee as u128)
            .ok_or(ErrorCode::NumericalOverflow)?
            .checked_div(100)
            .ok_or(ErrorCode::NumericalOverflow)? as u64;

        if creator_fee + creator_account.lamports() >= Rent::get()?.minimum_balance(0) {
            invoke(
                &system_instruction::transfer(
                    escrow_payment_account.key,
                    &creator_account.key(),
                    creator_fee,
                ),
                &[
                    escrow_payment_account.clone(),
                    creator_account.clone(),
                    system_program.clone(),
                ],
            )?;

            total_fee_paid = total_fee_paid
                .checked_add(creator_fee)
                .ok_or(ErrorCode::NumericalOverflow)?;
        }
    }

    if total_pct != 100 {
        return Err(ErrorCode::CreatorShareTotalMustBe100.into());
    }

    Ok(total_fee_paid)
}

pub fn verify_creators(
    creator_accounts: Iter<AccountInfo>,
    creator_shares: Vec<u16>,
    creator_verified: Vec<bool>,
    creator_hash: [u8; 32],
) -> Result<()> {
    // Check that all input arrays/vectors are of the same length
    if creator_accounts.len() != creator_shares.len()
        || creator_accounts.len() != creator_verified.len()
    {
        return Err(ErrorCode::MismatchedCreatorDataLengths.into());
    }

    // Convert input data to a vector of Creator structs
    let creators: Vec<Creator> = creator_accounts
        .zip(creator_shares.iter())
        .zip(creator_verified.iter())
        .map(|((account, &share), &verified)| Creator {
            address: *account.key,
            verified,
            share: share as u8, // Assuming the share is never more than 255. If it can be, this needs additional checks.
        })
        .collect();

    // Compute the hash from the Creator vector
    let computed_hash = hash_creators(&creators);

    // Compare the computed hash with the provided hash
    if computed_hash != creator_hash {
        msg!(
            "Computed hash does not match provided hash: {{\"computed\":{:?},\"provided\":{:?}}}",
            computed_hash,
            creator_hash
        );
        return Err(ErrorCode::InvalidCreators.into());
    }

    Ok(())
}

// Taken from Bubblegum's hash_metadata: hashes seller_fee_basis_points to the final data_hash that Bubblegum expects.
// This way we can use the seller_fee_basis_points while still guaranteeing validity.
pub fn hash_metadata_data(
    metadata_args_hash: [u8; 32],
    seller_fee_basis_points: u16,
) -> Result<[u8; 32]> {
    Ok(keccak::hashv(&[&metadata_args_hash, &seller_fee_basis_points.to_le_bytes()]).to_bytes())
}

pub struct FeeResults {
    pub maker_fee: i64,
    pub taker_fee: u64,
    pub seller_will_get_from_buyer: u64,
    pub total_platform_fee: u64,
}

pub fn calculate_fees(
    notary: &AccountInfo,
    buyer_price: u64,
    maker_fee_bp: i16,
    taker_fee_bp: u16,
    payer: &AccountInfo,
    seller: &AccountInfo,
) -> Result<FeeResults> {
    let (actual_maker_fee_bp, actual_taker_fee_bp) =
        get_actual_maker_taker_fee_bp(notary, maker_fee_bp, taker_fee_bp);

    assert_valid_fees_bp(actual_maker_fee_bp, actual_taker_fee_bp.try_into().unwrap())?;

    let maker_fee = (buyer_price as i128)
        .checked_mul(actual_maker_fee_bp as i128)
        .ok_or(ErrorCode::NumericalOverflow)?
        .checked_div(10000)
        .ok_or(ErrorCode::NumericalOverflow)? as i64;
    let taker_fee = (buyer_price as u128)
        .checked_mul(actual_taker_fee_bp as u128)
        .ok_or(ErrorCode::NumericalOverflow)?
        .checked_div(10000)
        .ok_or(ErrorCode::NumericalOverflow)? as u64;
    let seller_will_get_from_buyer = if payer.key.eq(seller.key) {
        (buyer_price as i64)
            .checked_add(maker_fee)
            .ok_or(ErrorCode::NumericalOverflow)?
    } else {
        (buyer_price as i64)
            .checked_sub(maker_fee)
            .ok_or(ErrorCode::NumericalOverflow)?
    } as u64;
    let total_platform_fee = u64::try_from(
        maker_fee
            .checked_add(taker_fee as i64)
            .ok_or(ErrorCode::NumericalOverflow)?,
    )
    .map_err(|_| ErrorCode::NumericalOverflow)?;

    Ok(FeeResults {
        maker_fee,
        taker_fee,
        seller_will_get_from_buyer,
        total_platform_fee,
    })
}

pub fn get_actual_maker_taker_fee_bp(
    notary: &AccountInfo,
    maker_fee_bp: i16,
    taker_fee_bp: u16,
) -> (i16, u16) {
    match notary.is_signer {
        true => (maker_fee_bp, taker_fee_bp),
        false => (DEFAULT_MAKER_FEE_BP, DEFAULT_TAKER_FEE_BP),
    }
}

pub fn assert_valid_fees_bp(maker_fee_bp: i16, taker_fee_bp: i16) -> Result<()> {
    let bound = MAX_FEE_ABS_BP;
    if !(0..=bound).contains(&taker_fee_bp) {
        return Err(ErrorCode::InvalidMakerTakerFee.into());
    }

    if !(-bound..=bound).contains(&maker_fee_bp) {
        return Err(ErrorCode::InvalidMakerTakerFee.into());
    }

    let sum = maker_fee_bp + taker_fee_bp;
    if !(0..=bound).contains(&sum) {
        return Err(ErrorCode::InvalidMakerTakerFee.into());
    }

    Ok(())
}
