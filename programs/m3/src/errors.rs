use anchor_lang::prelude::*;

#[error_code]
pub enum ErrorCode {
    // 6001
    #[msg("IncorrectOwner")]
    IncorrectOwner,
    // 6002
    #[msg("Asset ID does not match expected ID")]
    AssetIDMismatch,
    // 6003
    #[msg("No valid signer present")]
    NoValidSignerPresent,
    // 6004
    #[msg("Invalid notary")]
    InvalidNotary,
    // 6005
    #[msg("Empty trade state")]
    EmptyTradeState,
    // 6006
    #[msg("Invalid price")]
    InvalidPrice,
    // 6007
    #[msg("Invalid discriminator")]
    InvalidDiscriminator,
    // 6008
    #[msg("Invalid platform fee bp")]
    InvalidPlatformFeeBp,
    // 6009
    #[msg("NumericalOverflow")]
    NumericalOverflow,
    // 6010
    #[msg("MismatchedCreatorDataLengths")]
    MismatchedCreatorDataLengths,
    // 6011
    #[msg("CreatorShareTotalMustBe100")]
    CreatorShareTotalMustBe100,
    // 6012
    #[msg("InvalidMakerTakerFee")]
    InvalidMakerTakerFee,
    // 6013
    #[msg("InvalidCreators")]
    InvalidCreators,
    // 6014
    #[msg("SellerMismatch")]
    SellerMismatch,
}
