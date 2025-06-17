use anchor_lang::prelude::*;
use anchor_spl::{
    token::{self, Token, TokenAccount, Transfer, Mint},
    associated_token::{self, get_associated_token_address},
};
declare_id!("PbQHuZDEW5EUHo3tbRAcRSqsHqKCxBhQMBV197ZsTiz");
#[program]
pub mod reptilx_contract {
    use super::*;
    pub fn initialize_config(ctx: Context<InitializeConfig>, price_per_token: u64, sol_recipient: Pubkey) -> Result<()> {
        let config = &mut ctx.accounts.config;
        config.price_per_token = price_per_token;
        config.seller = ctx.accounts.seller.key();
        config.sol_recipient = sol_recipient;
        Ok(())
    }
    pub fn reset_config(ctx: Context<ResetConfig>, new_price: u64, new_recipient: Pubkey) -> Result<()> {
        let config = &mut ctx.accounts.config;
        require_keys_eq!(ctx.accounts.seller.key(), config.seller, CustomError::Unauthorized);
        config.price_per_token = new_price;
        config.sol_recipient = new_recipient;
        config.paused = false;
        Ok(())
    }
    pub fn update_price(ctx: Context<UpdatePrice>, new_price: u64) -> Result<()> {
        let config = &mut ctx.accounts.config;
        require_keys_eq!(ctx.accounts.seller.key(), config.seller, CustomError::Unauthorized);
        config.price_per_token = new_price;
        Ok(())
    }
    pub fn pause(ctx: Context<UpdatePrice>) -> Result<()> {
        let config = &mut ctx.accounts.config;
        require_keys_eq!(ctx.accounts.seller.key(), config.seller, CustomError::Unauthorized);
        config.paused = true;
        Ok(())
    }
    pub fn unpause(ctx: Context<UpdatePrice>) -> Result<()> {
        let config = &mut ctx.accounts.config;
        require_keys_eq!(ctx.accounts.seller.key(), config.seller, CustomError::Unauthorized);
        config.paused = false;
        Ok(())
    }
    pub fn buy(ctx: Context<Buy>, amount_of_spl: u64) -> Result<()> {
        require!(!ctx.accounts.config.paused, CustomError::SalePaused);
        require_keys_eq!(
            ctx.accounts.seller_spl_account.mint,
            ctx.accounts.mint.key(),
            CustomError::InvalidMint
        );
        require_keys_eq!(
            ctx.accounts.seller_spl_account.owner,
            ctx.accounts.pda_authority.key(),
            CustomError::InvalidAuthority
        );
        require_keys_eq!(
            ctx.accounts.sol_recipient.key(),
            ctx.accounts.config.sol_recipient,
            CustomError::InvalidSolRecipient
        );
        require!(amount_of_spl > 0, CustomError::InvalidAmount);
        let price_per_token = ctx.accounts.config.price_per_token;
        let total_sol = amount_of_spl
            .checked_mul(price_per_token)
            .ok_or(CustomError::Overflow)?
            .checked_div(1_000_000_000)
            .ok_or(CustomError::Overflow)?;
        if ctx.accounts.buyer_spl_account.owner == &System::id() {
            let expected_ata = get_associated_token_address(
                &ctx.accounts.buyer_wallet.key(),
                &ctx.accounts.mint.key(),
            );
            require_keys_eq!(
                expected_ata,
                ctx.accounts.buyer_spl_account.key(),
                CustomError::InvalidATA
            );
            let cpi_ctx = CpiContext::new(
                ctx.accounts.associated_token_program.to_account_info(),
                anchor_spl::associated_token::Create {
                    payer: ctx.accounts.buyer_wallet.to_account_info(),
                    associated_token: ctx.accounts.buyer_spl_account.to_account_info(),
                    authority: ctx.accounts.buyer_wallet.to_account_info(),
                    mint: ctx.accounts.mint.to_account_info(),
                    system_program: ctx.accounts.system_program.to_account_info(),
                    token_program: ctx.accounts.token_program.to_account_info(),
                },
            );
            associated_token::create(cpi_ctx)?;
        }
        let bump = ctx.bumps.pda_authority;
        let signer_seeds: &[&[u8]] = &[b"authority", &[bump]];
        let signer = &[&signer_seeds[..]];

        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            Transfer {
                from: ctx.accounts.seller_spl_account.to_account_info(),
                to: ctx.accounts.buyer_spl_account.to_account_info(),
                authority: ctx.accounts.pda_authority.to_account_info(),
            },
            signer,
        );
        token::transfer(cpi_ctx, amount_of_spl)?;
        anchor_lang::solana_program::program::invoke(
            &anchor_lang::solana_program::system_instruction::transfer(
                &ctx.accounts.buyer_wallet.key(),
                &ctx.accounts.sol_recipient.key(),
                total_sol,
            ),
            &[
                ctx.accounts.buyer_wallet.to_account_info(),
                ctx.accounts.sol_recipient.to_account_info(),
                ctx.accounts.system_program.to_account_info(),
            ],
        )?;

        Ok(())
    }
}
#[derive(Accounts)]
#[instruction(price_per_token: u64, sol_recipient: Pubkey)]
pub struct InitializeConfig<'info> {
    #[account(
        init_if_needed,
        payer = seller,
        space = 8 + 8 + 32 + 32 + 1,
        seeds = [b"config"],
        bump
    )]
    pub config: Account<'info, Config>,
    #[account(mut)]
    pub seller: Signer<'info>,
    pub system_program: Program<'info, System>,
}
#[derive(Accounts)]
pub struct ResetConfig<'info> {
    #[account(mut, seeds = [b"config"], bump)]
    pub config: Account<'info, Config>,
    pub seller: Signer<'info>,
}
#[derive(Accounts)]
pub struct UpdatePrice<'info> {
    #[account(mut, seeds = [b"config"], bump)]
    pub config: Account<'info, Config>,
    pub seller: Signer<'info>,
}
#[derive(Accounts)]
pub struct Buy<'info> {
    #[account(mut)]
    pub seller_spl_account: Account<'info, TokenAccount>,
    #[account(mut)]
    pub buyer_wallet: Signer<'info>,
    /// CHECK: Will be initialized if needed as associated token account, verified against expected ATA
    #[account(mut)]
    pub buyer_spl_account: UncheckedAccount<'info>,
    /// CHECK: Verified against config.sol_recipient in logic
    #[account(mut)]
    pub sol_recipient: AccountInfo<'info>,
    /// CHECK: Verified as PDA and owner of seller_spl_account
    #[account(seeds = [b"authority"], bump)]
    pub pda_authority: UncheckedAccount<'info>,
    #[account(seeds = [b"config"], bump)]
    pub config: Account<'info, Config>,
    pub mint: Account<'info, Mint>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub associated_token_program: Program<'info, anchor_spl::associated_token::AssociatedToken>,
}
#[account]
pub struct Config {
    pub price_per_token: u64,
    pub seller: Pubkey,
    pub sol_recipient: Pubkey,
    pub paused: bool,
}
#[error_code]
pub enum CustomError {
    #[msg("You are not authorized to update the price.")]
    Unauthorized,
    #[msg("Overflow occurred while calculating SOL cost.")]
    Overflow,
    #[msg("Associated Token Account does not match expected address.")]
    InvalidATA,
    #[msg("SPL Token mint does not match expected mint.")]
    InvalidMint,
    #[msg("Seller SPL token account is not owned by PDA authority.")]
    InvalidAuthority,
    #[msg("Invalid token amount specified.")]
    InvalidAmount,
    #[msg("SOL recipient does not match configuration.")]
    InvalidSolRecipient,
    #[msg("Token sale is currently paused.")]
    SalePaused,
}