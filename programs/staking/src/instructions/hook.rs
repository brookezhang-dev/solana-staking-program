//! Transfer Hook (v3.x, pool-scoped). This program IS the hook for each pool's $STAKE
//! mint. On every transfer Token-2022 CPIs `execute`, which settles BOTH parties'
//! MasterChef rewards against the POOL (strategy B: record only). Interest does not
//! travel with the token.
//!
//! ⚠ VERSION-SENSITIVE: spl-transfer-hook-interface 0.9 / spl-tlv-account-resolution 0.9.

use crate::constants::*;
use crate::errors::StakingError;
use crate::instructions::reward;
use crate::state::{Pool, UserInfo};
use anchor_lang::prelude::*;
use anchor_lang::system_program::{create_account, CreateAccount};
use anchor_spl::token_2022::spl_token_2022::extension::{
    transfer_hook::TransferHookAccount, BaseStateWithExtensions, StateWithExtensions,
};
use anchor_spl::token_2022::spl_token_2022::state::Account as Token2022Account;
use anchor_spl::token_interface::{Mint, TokenAccount};
use spl_tlv_account_resolution::{account::ExtraAccountMeta, seeds::Seed, state::ExtraAccountMetaList};
use spl_transfer_hook_interface::instruction::ExecuteInstruction;

// ---------------------------------------------------------------------------
// initialize_extra_account_meta_list — once per pool's $STAKE mint.
// Declares extras resolved at transfer time:
//   [5] pool (mut)                fixed pubkey
//   [6] source UserInfo (mut)     seeds [b"user_info", pool, source_token(idx 0)]
//   [7] destination UserInfo (mut) seeds [b"user_info", pool, dest_token(idx 2)]
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct InitializeExtraAccountMetaList<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    /// CHECK: PDA created & written here; validated by seeds.
    #[account(mut, seeds = [EXTRA_META_SEED, stake_mint.key().as_ref()], bump)]
    pub extra_account_meta_list: AccountInfo<'info>,

    #[account(constraint = pool.stake_receipt_mint == stake_mint.key() @ StakingError::PoolConfigMismatch)]
    pub pool: Account<'info, Pool>,

    #[account(address = pool.stake_receipt_mint)]
    pub stake_mint: InterfaceAccount<'info, Mint>,

    pub system_program: Program<'info, System>,
}

pub fn init_extra_metas_handler(ctx: Context<InitializeExtraAccountMetaList>) -> Result<()> {
    let metas = vec![
        ExtraAccountMeta::new_with_pubkey(&ctx.accounts.pool.key(), false, true)?, // [5] pool
        ExtraAccountMeta::new_with_seeds( // [6] source user_info
            &[
                Seed::Literal { bytes: USER_SEED.to_vec() },
                Seed::AccountKey { index: 5 },
                Seed::AccountKey { index: 0 },
            ],
            false,
            true,
        )?,
        ExtraAccountMeta::new_with_seeds( // [7] destination user_info
            &[
                Seed::Literal { bytes: USER_SEED.to_vec() },
                Seed::AccountKey { index: 5 },
                Seed::AccountKey { index: 2 },
            ],
            false,
            true,
        )?,
    ];

    let account_size = ExtraAccountMetaList::size_of(metas.len())?;
    let lamports = Rent::get()?.minimum_balance(account_size);
    let mint_key = ctx.accounts.stake_mint.key();
    let signer_seeds: &[&[&[u8]]] = &[&[EXTRA_META_SEED, mint_key.as_ref(), &[ctx.bumps.extra_account_meta_list]]];
    create_account(
        CpiContext::new_with_signer(
            ctx.accounts.system_program.to_account_info(),
            CreateAccount {
                from: ctx.accounts.payer.to_account_info(),
                to: ctx.accounts.extra_account_meta_list.to_account_info(),
            },
            signer_seeds,
        ),
        lamports,
        account_size as u64,
        &crate::ID,
    )?;
    ExtraAccountMetaList::init::<ExecuteInstruction>(
        &mut ctx.accounts.extra_account_meta_list.try_borrow_mut_data()?,
        &metas,
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// execute — invoked by Token-2022 on every $STAKE transfer.
// ---------------------------------------------------------------------------

#[derive(Accounts)]
pub struct TransferHook<'info> {
    #[account(token::mint = mint)]
    pub source_token: InterfaceAccount<'info, TokenAccount>,
    pub mint: InterfaceAccount<'info, Mint>,
    #[account(token::mint = mint)]
    pub destination_token: InterfaceAccount<'info, TokenAccount>,
    /// CHECK: source authority; not dereferenced.
    pub owner: UncheckedAccount<'info>,
    /// CHECK: ExtraAccountMetaList PDA.
    #[account(seeds = [EXTRA_META_SEED, mint.key().as_ref()], bump)]
    pub extra_account_meta_list: UncheckedAccount<'info>,

    // ---- extras (resolver order) ----
    #[account(mut, constraint = pool.stake_receipt_mint == mint.key() @ StakingError::PoolConfigMismatch)]
    pub pool: Account<'info, Pool>,
    #[account(mut, seeds = [USER_SEED, pool.key().as_ref(), source_token.key().as_ref()], bump)]
    pub source_user_info: Account<'info, UserInfo>,
    #[account(mut, seeds = [USER_SEED, pool.key().as_ref(), destination_token.key().as_ref()], bump)]
    pub destination_user_info: Account<'info, UserInfo>,
}

fn assert_transferring(ai: &AccountInfo) -> Result<()> {
    let data = ai.try_borrow_data()?;
    let state = StateWithExtensions::<Token2022Account>::unpack(&data)
        .map_err(|_| error!(StakingError::NotTransferring))?;
    let ext = state
        .get_extension::<TransferHookAccount>()
        .map_err(|_| error!(StakingError::NotTransferring))?;
    require!(bool::from(ext.transferring), StakingError::NotTransferring);
    Ok(())
}

pub fn execute_handler(ctx: Context<TransferHook>, amount: u64) -> Result<()> {
    assert_transferring(&ctx.accounts.source_token.to_account_info())?;

    let now = Clock::get()?.unix_timestamp;
    reward::update_pool(&mut ctx.accounts.pool, now)?;
    let acc = ctx.accounts.pool.acc_reward_per_share;

    // Token-2022 already moved the tokens: balances are POST-transfer.
    let src_post = ctx.accounts.source_token.amount;
    let dst_post = ctx.accounts.destination_token.amount;
    let src_pre = src_post.checked_add(amount).ok_or(StakingError::MathOverflow)?;
    let dst_pre = dst_post.checked_sub(amount).ok_or(StakingError::MathOverflow)?;

    {
        let ui = &mut ctx.accounts.source_user_info;
        let pending = reward::pending_reward(src_pre, acc, ui.reward_debt)?;
        ui.pending_unclaimed = ui.pending_unclaimed.checked_add(pending).ok_or(StakingError::MathOverflow)?;
        ui.reward_debt = reward::reward_debt_for(src_post, acc)?;
    }
    {
        let ui = &mut ctx.accounts.destination_user_info;
        let pending = reward::pending_reward(dst_pre, acc, ui.reward_debt)?;
        ui.pending_unclaimed = ui.pending_unclaimed.checked_add(pending).ok_or(StakingError::MathOverflow)?;
        ui.reward_debt = reward::reward_debt_for(dst_post, acc)?;
    }
    // total_staked unchanged across a transfer.
    Ok(())
}
