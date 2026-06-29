//! create_token_metadata: attach Metaplex Token Metadata to a program-controlled
//! mint ($STAKE or $MILK) so wallets/explorers show a name + symbol instead of
//! "Unknown token". The mint authority is the Config PDA, so the metadata-creation
//! CPI must be signed by the program with the Config PDA seeds.
//!
//! $BEEF is the external input token (authority = its own owner), so its metadata
//! is out of scope here.

use crate::constants::*;
use crate::errors::StakingError;
use crate::state::Config;
use anchor_lang::prelude::*;
use anchor_spl::metadata::{
    create_metadata_accounts_v3, mpl_token_metadata::types::DataV2,
    CreateMetadataAccountsV3, Metadata,
};
use anchor_spl::token::Mint;

#[derive(Accounts)]
pub struct CreateTokenMetadata<'info> {
    #[account(mut, address = config.admin @ StakingError::Unauthorized)]
    pub admin: Signer<'info>,

    #[account(seeds = [CONFIG_SEED], bump = config.bump)]
    pub config: Account<'info, Config>,

    // Only the program-controlled mints (authority = Config PDA) are allowed.
    #[account(
        mut,
        constraint = (mint.key() == config.stake_mint || mint.key() == config.milk_mint)
            @ StakingError::InvalidMint
    )]
    pub mint: Account<'info, Mint>,

    /// CHECK: PDA validated by the Token Metadata program (seeds = ["metadata", program, mint]).
    #[account(mut)]
    pub metadata: UncheckedAccount<'info>,

    pub token_metadata_program: Program<'info, Metadata>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

pub fn handler(
    ctx: Context<CreateTokenMetadata>,
    name: String,
    symbol: String,
    uri: String,
) -> Result<()> {
    let signer_seeds: &[&[&[u8]]] = &[&[CONFIG_SEED, &[ctx.accounts.config.bump]]];

    let data = DataV2 {
        name,
        symbol,
        uri,
        seller_fee_basis_points: 0,
        creators: None,
        collection: None,
        uses: None,
    };

    create_metadata_accounts_v3(
        CpiContext::new_with_signer(
            ctx.accounts.token_metadata_program.to_account_info(),
            CreateMetadataAccountsV3 {
                metadata: ctx.accounts.metadata.to_account_info(),
                mint: ctx.accounts.mint.to_account_info(),
                mint_authority: ctx.accounts.config.to_account_info(),
                update_authority: ctx.accounts.config.to_account_info(),
                payer: ctx.accounts.admin.to_account_info(),
                system_program: ctx.accounts.system_program.to_account_info(),
                rent: ctx.accounts.rent.to_account_info(),
            },
            signer_seeds,
        ),
        data,
        true, // is_mutable
        true, // update_authority_is_signer (Config PDA signs via seeds)
        None, // collection_details
    )?;

    Ok(())
}
