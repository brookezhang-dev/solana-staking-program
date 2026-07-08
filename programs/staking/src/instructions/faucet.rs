//! faucet (v3, devnet-only): mint capped test $BEEF to the caller so any new
//! wallet can try staking. Compiled ONLY under `feature = "devnet-faucet"`;
//! main-net build omits it entirely (not a runtime switch). Requires $BEEF mint
//! authority = Config PDA (devnet setup arranges this).

use crate::constants::*;
use crate::errors::StakingError;
use crate::state::Config;
use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self, Mint, MintTo, TokenAccount, TokenInterface};

#[derive(Accounts)]
pub struct Faucet<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Account<'info, Config>,

    #[account(mut, address = config.beef_mint)]
    pub beef_mint: InterfaceAccount<'info, Mint>,
    #[account(mut, token::mint = beef_mint, token::authority = user)]
    pub user_beef_ata: InterfaceAccount<'info, TokenAccount>,

    pub beef_token_program: Interface<'info, TokenInterface>,
}

pub fn handler(ctx: Context<Faucet>, amount: u64) -> Result<()> {
    require!(amount > 0, StakingError::AmountZero);
    require!(amount <= FAUCET_MAX, StakingError::FaucetTooMuch);

    let signer: &[&[&[u8]]] = &[&[CONFIG_SEED, &[ctx.accounts.config.bump]]];
    token_interface::mint_to(
        CpiContext::new_with_signer(
            ctx.accounts.beef_token_program.to_account_info(),
            MintTo {
                mint: ctx.accounts.beef_mint.to_account_info(),
                to: ctx.accounts.user_beef_ata.to_account_info(),
                authority: ctx.accounts.config.to_account_info(),
            },
            signer,
        ),
        amount,
    )?;
    Ok(())
}
