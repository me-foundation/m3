use solana_program::{pubkey, pubkey::Pubkey};

pub const PREFIX: &str = "m3";
pub const SIGNER: &str = "signer";
pub const MAX_PRICE: u64 = 8000000 * 1000000000;
pub const MAX_FEE_ABS_BP: i16 = 500;
// Priviledged account for cancelling
pub const CANCEL_AUTHORITY: Pubkey = if cfg!(feature = "anchor-test") {
    pubkey!("CcQQ9E8N1YDLY7dRffTzyACRPUDyv94UGdy6H2uiHycB")
} else {
    pubkey!("CNTuB1JiQD8Xh5SoRcEmF61yivN9F7uzdSaGnRex36wi")
};

// Same one as M2 auctionhouse
pub const ME_TREASURY: Pubkey = pubkey!("rFqFJ9g7TGBD8Ed7TPDnvGKZ5pWLPDyxLcvcH2eRCtt");

pub const ME_NOTARY: Pubkey = if cfg!(feature = "anchor-test") {
    pubkey!("NsqaguhBJckevdkfGx3wrCEg3581AdpfFCZdmLkBmV9")
} else {
    pubkey!("NTYeYJ1wr4bpM5xo6zx5En44SvJFAd35zTxxNoERYqd")
};

pub const DEFAULT_MAKER_FEE_BP: i16 = 0;
pub const DEFAULT_TAKER_FEE_BP: u16 = 250;

// Enforce 100% royalty for creators paid by buyer
pub const DEFAULT_CREATOR_ROYALTY_BP: u16 = 100 * 100;
