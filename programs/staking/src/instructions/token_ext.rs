//! Token-2022 extension checks used by `initialize` (v3 §9.2 / §9.4).
//!
//! ⚠ HIGH-RISK / VERSION-SENSITIVE: the spl-token-2022 extension-parsing API
//! moves between versions. If `anchor build` errors here, adjust the import paths
//! to match the spl-token-2022 pulled in by anchor-spl 0.31 (`cargo tree | grep
//! spl-token-2022`). Logic is intentionally small so it is easy to fix.

use crate::errors::StakingError;
use anchor_lang::prelude::*;
use anchor_spl::token_2022::spl_token_2022::{
    extension::{BaseStateWithExtensions, ExtensionType, StateWithExtensions},
    state::Mint as MintState,
};

/// Require the mint to be a Token-2022 NonTransferable mint (soulbound).
pub fn require_non_transferable(mint_ai: &AccountInfo) -> Result<()> {
    let data = mint_ai.try_borrow_data()?;
    let mint = StateWithExtensions::<MintState>::unpack(&data)
        .map_err(|_| error!(StakingError::StakeMintNotNonTransferable))?;
    let exts = mint
        .get_extension_types()
        .map_err(|_| error!(StakingError::StakeMintNotNonTransferable))?;
    require!(
        exts.contains(&ExtensionType::NonTransferable),
        StakingError::StakeMintNotNonTransferable
    );
    Ok(())
}

/// Reject mints carrying a TransferHook extension (unsupported, v3 §9.4).
/// TransferFee is allowed — handled by balance-diff accounting (§9.3).
/// Classic SPL mints have no extensions and pass trivially.
pub fn require_no_transfer_hook(mint_ai: &AccountInfo) -> Result<()> {
    let data = mint_ai.try_borrow_data()?;
    if let Ok(mint) = StateWithExtensions::<MintState>::unpack(&data) {
        if let Ok(exts) = mint.get_extension_types() {
            require!(
                !exts.contains(&ExtensionType::TransferHook),
                StakingError::UnsupportedTokenExtension
            );
        }
    }
    Ok(())
}
