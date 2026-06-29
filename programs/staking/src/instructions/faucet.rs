//! faucet: devnet convenience — mint test $BEEF to the caller so any new wallet
//! can try staking. Permissionless, capped per call (FAUCET_MAX). Requires the
//! $BEEF mint authority to be the Config PDA (see scripts/beef-authority-to-pda.ts).
//!
//! NOTE: this exists only because $BEEF is a throwaway test token on devnet. A
//! real input token would have its own supply/issuance and no such faucet.

use crate::constants::*;
use crate::errors::StakingError;
use crate::state::Config;
use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, MintTo, Token, TokenAccount};

#[derive(Accounts)]
pub struct Faucet<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Account<'info, Config>,

    #[account(mut, address = config.beef_mint)]
    pub beef_mint: Account<'info, Mint>,

    #[account(mut, constraint = user_beef_ata.mint == config.beef_mint @ StakingError::InvalidMint)]
    pub user_beef_ata: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

pub fn handler(ctx: Context<Faucet>, amount: u64) -> Result<()> {
    require!(amount > 0, StakingError::AmountZero);
    require!(amount <= FAUCET_MAX, StakingError::FaucetTooMuch);

    // Mint $BEEF to the caller (Config PDA signs as the $BEEF mint authority).
    let signer_seeds: &[&[&[u8]]] = &[&[CONFIG_SEED, &[ctx.accounts.config.bump]]];
    token::mint_to(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            MintTo {
                mint: ctx.accounts.beef_mint.to_account_info(),
                to: ctx.accounts.user_beef_ata.to_account_info(),
                authority: ctx.accounts.config.to_account_info(),
            },
            signer_seeds,
        ),
        amount,
    )?;
    Ok(())
}
