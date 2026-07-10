//! Solana DeFi staking program (Anchor, v3.x — Config/Pool two-tier).
//! Tier 1: one protocol `Config` (admin, pool_count, global pause).
//! Tier 2: N `Pool`s, one per (staked_mint, reward_mint) pair, created on demand.
//! $STAKE is a transferable Token-2022 TransferHook certificate (balance = ledger);
//! MasterChef O(1) rewards paid from each pool's prefunded reward vault (strategy B);
//! rewards settle on every transfer via the program's own hook.

use anchor_lang::prelude::*;
use spl_transfer_hook_interface::instruction::TransferHookInstruction;

pub mod constants;
pub mod errors;
pub mod events;
pub mod instructions;
pub mod state;

use instructions::*;

declare_id!("54HWhVGu8HoK46PUj3ijauVjrgNScGHyzpnvHsvZGpcv");

#[program]
pub mod staking {
    use super::*;

    // ---- Tier 1: protocol ----
    pub fn initialize_config(ctx: Context<InitializeConfig>) -> Result<()> {
        instructions::initialize::handler(ctx)
    }
    pub fn set_pause(ctx: Context<AdminConfig>, paused: bool) -> Result<()> {
        instructions::admin::set_pause_handler(ctx, paused)
    }
    pub fn transfer_admin(ctx: Context<AdminConfig>, new_admin: Pubkey) -> Result<()> {
        instructions::admin::transfer_admin_handler(ctx, new_admin)
    }

    // ---- Tier 2: pool lifecycle (admin) ----
    pub fn create_pool(ctx: Context<CreatePool>, reward_per_sec: u64, start_time: i64, end_time: i64) -> Result<()> {
        instructions::create_pool::handler(ctx, reward_per_sec, start_time, end_time)
    }
    pub fn set_emission(ctx: Context<SetEmission>, reward_per_sec: u64, end_time: i64) -> Result<()> {
        instructions::admin::set_emission_handler(ctx, reward_per_sec, end_time)
    }
    pub fn withdraw_surplus(ctx: Context<WithdrawSurplus>, amount: u64) -> Result<()> {
        instructions::admin::withdraw_surplus_handler(ctx, amount)
    }
    pub fn initialize_extra_account_meta_list(ctx: Context<InitializeExtraAccountMetaList>) -> Result<()> {
        instructions::hook::init_extra_metas_handler(ctx)
    }

    // ---- user ----
    pub fn stake(ctx: Context<Stake>, amount: u64) -> Result<()> {
        instructions::stake::handler(ctx, amount)
    }
    pub fn unstake(ctx: Context<Unstake>, amount: u64) -> Result<()> {
        instructions::unstake::handler(ctx, amount)
    }
    pub fn claim_rewards(ctx: Context<ClaimRewards>) -> Result<()> {
        instructions::claim::handler(ctx)
    }
    pub fn fund_rewards(ctx: Context<FundRewards>, amount: u64) -> Result<()> {
        instructions::fund::handler(ctx, amount)
    }
    pub fn register(ctx: Context<Register>) -> Result<()> {
        instructions::register::handler(ctx)
    }
    pub fn close_user_info(ctx: Context<CloseUserInfo>) -> Result<()> {
        instructions::close_user::handler(ctx)
    }

    // ---- transfer hook execute (dispatched via fallback) ----
    pub fn transfer_hook(ctx: Context<TransferHook>, amount: u64) -> Result<()> {
        instructions::hook::execute_handler(ctx, amount)
    }

    // ---- devnet-only faucet ----
    #[cfg(feature = "devnet-faucet")]
    pub fn faucet(ctx: Context<Faucet>, amount: u64) -> Result<()> {
        instructions::faucet::handler(ctx, amount)
    }

    /// Route Token-2022's TransferHook `Execute` (spl-transfer-hook-interface
    /// discriminator) to our `transfer_hook` handler.
    pub fn fallback<'info>(
        program_id: &Pubkey,
        accounts: &'info [AccountInfo<'info>],
        data: &[u8],
    ) -> Result<()> {
        match TransferHookInstruction::unpack(data)
            .map_err(|_| error!(errors::StakingError::NotTransferring))?
        {
            TransferHookInstruction::Execute { amount } => {
                let amount_bytes = amount.to_le_bytes();
                __private::__global::transfer_hook(program_id, accounts, &amount_bytes)
            }
            _ => Err(anchor_lang::solana_program::program_error::ProgramError::InvalidInstructionData.into()),
        }
    }
}
