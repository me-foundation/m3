#![allow(clippy::result_large_err)]

mod constants;
mod errors;
mod ins;
mod states;
mod utils;

use crate::ins::*;
use anchor_lang::prelude::*;

anchor_lang::declare_id!("M3mxk5W2tt27WGT7THox7PmgRDp4m6NEhL5xvxrBfS1");

#[program]
pub mod m3 {
    use super::*;

    pub fn sell<'info>(ctx: Context<'_, '_, '_, 'info, Sell<'info>>, args: SellArgs) -> Result<()> {
        ins::sell::handle(ctx, args)
    }

    pub fn buy_now<'info>(
        ctx: Context<'_, '_, '_, 'info, BuyNow<'info>>,
        args: BuyNowArgs,
    ) -> Result<()> {
        ins::buy_now::handle(ctx, args)
    }

    pub fn cancel_sell<'info>(
        ctx: Context<'_, '_, '_, 'info, CancelSell<'info>>,
        args: CancelSellArgs,
    ) -> Result<()> {
        ins::cancel_sell::handle(ctx, args)
    }
}
