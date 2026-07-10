//! Token-2022 extension checks used by `initialize` (v3.1).
//!
//! ⚠ HIGH-RISK / VERSION-SENSITIVE: the spl-token-2022 extension-parsing API
//! moves between versions. If `anchor build` errors here, adjust the import paths
//! to match the spl-token-2022 pulled in by anchor-spl 0.31 (`cargo tree | grep
//! spl-token-2022`). Logic is intentionally small so it is easy to fix.

use crate::errors::StakingError;
use anchor_lang::prelude::*;
use anchor_spl::token_2022::spl_token_2022::{
    extension::{transfer_hook::TransferHook as TransferHookExt, BaseStateWithExtensions, ExtensionType, StateWithExtensions},
    state::Mint as MintState,
};

/// v3.1: require the $STAKE mint to carry a TransferHook extension whose
/// program_id points at THIS program, so every transfer routes back to our hook.
pub fn require_transfer_hook_to(mint_ai: &AccountInfo, program_id: &Pubkey) -> Result<()> {
    let data = mint_ai.try_borrow_data()?;
    let mint = StateWithExtensions::<MintState>::unpack(&data)
        .map_err(|_| error!(StakingError::StakeMintHookMismatch))?;
    let ext = mint
        .get_extension::<TransferHookExt>()
        .map_err(|_| error!(StakingError::StakeMintHookMismatch))?;
    let hook_pid: Option<Pubkey> = ext.program_id.into();
    require!(hook_pid == Some(*program_id), StakingError::StakeMintHookMismatch);
    Ok(())
}

/// Reject $BEEF / reward mints carrying a TransferHook extension (unsupported).
/// TransferFee is allowed — handled by balance-diff accounting.
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
