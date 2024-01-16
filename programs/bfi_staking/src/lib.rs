use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{mint_to, transfer, Mint, MintTo, Token, TokenAccount, Transfer},
};
use solana_program::clock::Clock;
declare_id!("3gbCKLUwbRTeGKP12jvPFDP8H3jWPVgAeitrMG92k4KH");

pub mod constants {
    pub const VAULT_SEED: &[u8] = b"vault";
    pub const STATUS_SEED: &[u8] = b"status";
    pub const POOL_SEED: &[u8] = b"pool";
}

#[program]
pub mod bfi_staking {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        // Validate that status has not been initialized yet
        if ctx.accounts.status.token != Pubkey::default() {
            return err!(StakingErrors::AlreadyInitialized);
        }

        let status = &mut ctx.accounts.status;
        status.token = *ctx.accounts.mint.to_account_info().key;
        status.owner = *ctx.accounts.signer.to_account_info().key;

        Ok(())
    }

    pub fn stake(ctx: Context<Stake>, pool_id: u8, amount: u64) -> Result<()> {
        // Validate pool is active
        if !ctx.accounts.pool.is_active {
            return err!(StakingErrors::InactivePool);
        }
        // Validate that token is the same as the one in status account
        if ctx.accounts.mint.to_account_info().key != &ctx.accounts.status.token {
            return err!(StakingErrors::InvalidTokenAccount);
        }
        // Validate that pool_id is valid
        if ctx.accounts.status.total_pools < pool_id {
            return err!(StakingErrors::InvalidPoolId);
        }
        //
        // Validate that amount is not 0
        if amount == 0 {
            return err!(StakingErrors::NoTokens);
        }

        let clock = Clock::get()?;
        if ctx.accounts.staking_position.amount > 0 && !ctx.accounts.staking_position.claimed {
            let lock_end_time =
                ctx.accounts.staking_position.start_time + ctx.accounts.pool.lock_time;
            //claim rewards before staking again
            let mut reward = calc_rewards(
                ctx.accounts.staking_position.amount,
                ctx.accounts.pool.basis_points,
                clock.unix_timestamp - ctx.accounts.staking_position.start_time,
                ctx.accounts.pool.lock_time,
            );
            if reward > 0 {
                if clock.unix_timestamp > lock_end_time {
                    // if lock period is over: claim rewards and restake deposit amount
                    reward += ctx.accounts.staking_position.lock_amount;
                    ctx.accounts.staking_position.lock_amount = 0;
                    let t_res = mint_to(
                        CpiContext::new(
                            ctx.accounts.token_program.to_account_info(),
                            MintTo {
                                mint: ctx.accounts.mint.to_account_info(),
                                to: ctx.accounts.user_token_account.to_account_info(),
                                authority: ctx.accounts.token_vault.to_account_info(),
                            },
                        ),
                        reward,
                    );
                    if t_res.is_err() {
                        return err!(StakingErrors::MintError);
                    }
                } else {
                    // else lock period is not over: lock accumulated rewards and restake deposit amount with new amount
                    ctx.accounts.staking_position.lock_amount += reward;
                }
            }
        }
        if ctx.accounts.staking_position.claimed {
            ctx.accounts.staking_position.claimed = false;
            ctx.accounts.staking_position.amount = 0;
        }
        //-------UPDATE USER VALUES-------//
        ctx.accounts.status.total_staked += amount;
        ctx.accounts.staking_position.amount += amount;
        ctx.accounts.staking_position.start_time = clock.unix_timestamp;

        // Transfer tokens from user to vault
        let t_res = transfer(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.user_token_account.to_account_info(),
                    to: ctx.accounts.token_vault.to_account_info(),
                    authority: ctx.accounts.signer.to_account_info(),
                },
            ),
            amount,
        );
        if t_res.is_err() {
            return err!(StakingErrors::TransferError);
        }
        Ok(())
    }

    pub fn claim(ctx: Context<Stake>, pool_id: u8) -> Result<()> {
        // Validate that token is the same as the one in status account
        if ctx.accounts.mint.to_account_info().key != &ctx.accounts.status.token {
            return err!(StakingErrors::InvalidTokenAccount);
        }
        // Validate that pool_id is valid
        if ctx.accounts.status.total_pools < pool_id {
            return err!(StakingErrors::InvalidPoolId);
        }
        // validate user deposit is not 0
        if ctx.accounts.staking_position.amount == 0 {
            return err!(StakingErrors::NoTokens);
        }
        let lock_duration = ctx.accounts.pool.lock_time + ctx.accounts.staking_position.start_time;
        let clock = Clock::get()?;
        // validate user has not claimed yet or still under lock period
        if ctx.accounts.staking_position.claimed || clock.unix_timestamp < lock_duration {
            return err!(StakingErrors::NothingToClaim);
        }

        let reward = calc_rewards(
            ctx.accounts.staking_position.amount,
            ctx.accounts.pool.basis_points,
            1,
            1,
        ) + ctx.accounts.staking_position.lock_amount;
        msg!("Reward: {}", reward);
        // reset the locked amount
        ctx.accounts.staking_position.lock_amount = 0;
        ctx.accounts.status.total_staked -= ctx.accounts.staking_position.amount;
        // SET USER AS CLAIMED
        ctx.accounts.staking_position.claimed = true;
        msg!("Transferring tokens");
        let bump = ctx.bumps.token_vault;
        let vault_signer: &[&[&[u8]]] = &[&[constants::VAULT_SEED, &[bump]]];
        // Transfer tokens from vault to user
        let t_res = transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.token_vault.to_account_info(),
                    to: ctx.accounts.user_token_account.to_account_info(),
                    authority: ctx.accounts.token_vault.to_account_info(),
                },
                vault_signer,
            ),
            ctx.accounts.staking_position.amount,
        );
        if t_res.is_err() {
            return err!(StakingErrors::TransferError);
        }
        // MINT REWARDS
        let bump = ctx.bumps.token_vault;
        let vault_signer: &[&[&[u8]]] = &[&[constants::VAULT_SEED, &[bump]]];
        let t_res = mint_to(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                MintTo {
                    mint: ctx.accounts.mint.to_account_info(),
                    to: ctx.accounts.user_token_account.to_account_info(),
                    authority: ctx.accounts.token_vault.to_account_info(),
                },
                vault_signer,
            ),
            reward,
        );
        if t_res.is_err() {
            return err!(StakingErrors::MintError);
        }

        Ok(())
    }

    pub fn withdraw(ctx: Context<Stake>, pool_id: u8) -> Result<()> {
        // Validate that token is the same as the one in status account
        if ctx.accounts.mint.to_account_info().key != &ctx.accounts.status.token {
            return err!(StakingErrors::InvalidTokenAccount);
        }
        // Validate that pool_id is valid
        if ctx.accounts.status.total_pools < pool_id {
            return err!(StakingErrors::InvalidPoolId);
        }
        // validate user deposit is not 0
        if ctx.accounts.staking_position.amount == 0 {
            return err!(StakingErrors::NoTokens);
        }

        let mut deposited_amount = ctx.accounts.staking_position.amount;
        ctx.accounts.status.total_staked -= deposited_amount;
        ctx.accounts.staking_position.amount = 0;
        ctx.accounts.staking_position.lock_amount = 0;
        ctx.accounts.staking_position.claimed = false;
        ctx.accounts.staking_position.start_time = 0;

        let early_fee = deposited_amount * 5 / 100;
        deposited_amount -= early_fee;
        ctx.accounts.status.early_collected_fee += early_fee;

        let bump = ctx.bumps.token_vault;
        let vault_signer: &[&[&[u8]]] = &[&[constants::VAULT_SEED, &[bump]]];

        let t_res = transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.token_vault.to_account_info(),
                    to: ctx.accounts.user_token_account.to_account_info(),
                    authority: ctx.accounts.token_vault.to_account_info(),
                },
                vault_signer,
            ),
            deposited_amount,
        );
        if t_res.is_err() {
            return err!(StakingErrors::TransferError);
        }

        Ok(())
    }

    pub fn create_pool(
        ctx: Context<CreatePool>,
        pool_id: u8,
        basis_points: u32,
        lock_time: i64,
    ) -> Result<()> {
        if ctx.accounts.status.owner != *ctx.accounts.signer.to_account_info().key {
            return err!(StakingErrors::NotOwner);
        }
        if pool_id <= ctx.accounts.status.total_pools {
            return err!(StakingErrors::InvalidPoolId);
        }
        if basis_points == 0 || lock_time == 0 {
            return err!(StakingErrors::InvalidInputValues);
        }
        let pool_info = &mut ctx.accounts.new_pool;
        pool_info.basis_points = basis_points;
        pool_info.lock_time = lock_time;
        pool_info.is_active = true;
        // Increase total Pools by 1
        ctx.accounts.status.total_pools += 1;
        Ok(())
    }

    pub fn set_pool_active_status(
        ctx: Context<PoolUpdate>,
        pool_id: u8,
        active_state: bool,
    ) -> Result<()> {
        if ctx.accounts.status.owner != *ctx.accounts.signer.to_account_info().key {
            return err!(StakingErrors::NotOwner);
        }
        if pool_id > ctx.accounts.status.total_pools {
            return err!(StakingErrors::InvalidPoolId);
        }
        if ctx.accounts.pool.is_active == active_state {
            return Ok(());
        }
        ctx.accounts.pool.is_active = active_state;
        Ok(())
    }

    pub fn claim_early_fee(ctx: Context<ClaimEarlyFee>) -> Result<()> {
        if ctx.accounts.status.owner != *ctx.accounts.signer.to_account_info().key {
            return err!(StakingErrors::NotOwner);
        }
        let fee = ctx.accounts.status.early_collected_fee;
        if fee == 0 {
            return err!(StakingErrors::NothingToClaim);
        }
        ctx.accounts.status.early_collected_fee = 0;

        let bump = ctx.bumps.token_vault;
        let vault_signer: &[&[&[u8]]] = &[&[constants::VAULT_SEED, &[bump]]];

        let t_res = transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                Transfer {
                    from: ctx.accounts.token_vault.to_account_info(),
                    to: ctx.accounts.signer.to_account_info(),
                    authority: ctx.accounts.signer.to_account_info(),
                },
                vault_signer,
            ),
            fee,
        );
        if t_res.is_err() {
            return err!(StakingErrors::TransferError);
        }
        Ok(())
    }

    pub fn transfer_ownership(ctx: Context<TransferOwnership>) -> Result<()> {
        if ctx.accounts.status.owner != *ctx.accounts.signer.to_account_info().key {
            return err!(StakingErrors::NotOwner);
        }
        ctx.accounts.status.owner = *ctx.accounts.new_owner.key;
        Ok(())
    }
}
//------------------Helper Functions------------------//
fn calc_rewards(staked_amount: u64, basis_points: u32, time_dif: i64, total_diff: i64) -> u64 {
    let mut rewards = (staked_amount * basis_points as u64) / 100;
    if time_dif < 0 || total_diff < 0 {
        return 0;
    }
    if time_dif < total_diff {
        rewards = (rewards * time_dif as u64) / (total_diff as u64);
    }
    return rewards;
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    #[account(
        init_if_needed,
        seeds = [constants::VAULT_SEED],
        bump,
        payer = signer,
        token::mint = mint,
        token::authority = token_vault,
    )]
    pub token_vault: Account<'info, TokenAccount>,

    #[account(
        init_if_needed,
        seeds = [constants::STATUS_SEED],
        bump,
        payer = signer,
        space =  8 + std::mem::size_of::<Status>()
    )]
    pub status: Account<'info, Status>,

    pub mint: Account<'info, Mint>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(pool_id: u8)]
pub struct CreatePool<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    #[account(
        init_if_needed,
        seeds = [constants::POOL_SEED, &[pool_id]],
        bump,
        payer = signer,
        space = 8 + std::mem::size_of::<PoolInfo>()
    )]
    pub new_pool: Account<'info, PoolInfo>,

    #[account(mut)]
    pub status: Account<'info, Status>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(pool_id: u8)]
pub struct Stake<'info> {
    // Signer
    #[account(mut)]
    pub signer: Signer<'info>,

    // Pool Account
    #[account(
        seeds = [constants::POOL_SEED, &[pool_id]],
        bump,
    )]
    pub pool: Account<'info, PoolInfo>,

    // Status Account
    #[account(mut,
        seeds = [constants::STATUS_SEED],
        bump,
    )]
    pub status: Account<'info, Status>,

    // User's Token Account
    #[account(mut,
        associated_token::mint = mint,
        associated_token::authority = signer,
    )]
    pub user_token_account: Account<'info, TokenAccount>,

    // User's Staking Position
    #[account(
        init_if_needed,
        seeds = [signer.key.as_ref(), &[pool_id]],
        bump,
        payer = signer,
        space = 8 + std::mem::size_of::<StakingPosition>()
    )]
    pub staking_position: Account<'info, StakingPosition>,
    #[account(mut,
        seeds=[constants::VAULT_SEED],
        bump,
        token::mint = mint,
        token::authority = token_vault,
    )]
    pub token_vault: Account<'info, TokenAccount>,

    #[account(
        mut,
        mint::authority = token_vault,
    )]
    pub mint: Account<'info, Mint>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(pool_id: u8)]
pub struct PoolUpdate<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    #[account(
        mut,
        seeds = [constants::POOL_SEED, &[pool_id]],
        bump,
    )]
    pub pool: Account<'info, PoolInfo>,

    pub status: Account<'info, Status>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ClaimEarlyFee<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    #[account(mut,
        seeds=[constants::VAULT_SEED],
        bump,
        token::mint = mint,
        token::authority = token_vault,
    )]
    pub token_vault: Account<'info, TokenAccount>,

    #[account(mut,
        seeds=[constants::STATUS_SEED],
        bump,
    )]
    pub status: Account<'info, Status>,

    pub mint: Account<'info, Mint>,
    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct TransferOwnership<'info> {
    #[account(mut)]
    pub signer: Signer<'info>,

    #[account(mut,
        seeds=[constants::STATUS_SEED],
        bump,
    )]
    pub status: Account<'info, Status>,
    /// CHECK: Should be a valid User Wallet address
    pub new_owner: AccountInfo<'info>,
}

#[account]
pub struct Status {
    pub total_staked: u64,
    pub total_pools: u8,
    pub token: Pubkey,
    pub early_collected_fee: u64,
    pub owner: Pubkey,
}

#[account]
pub struct PoolInfo {
    pub basis_points: u32,
    pub lock_time: i64,
    pub is_active: bool,
}

#[account]
pub struct StakingPosition {
    pub amount: u64,
    pub start_time: i64,
    pub lock_amount: u64,
    pub claimed: bool,
}

#[error_code]
pub enum StakingErrors {
    #[msg("Check input values")]
    InvalidInputValues,
    #[msg("Invalid pool id")]
    InvalidPoolId,
    #[msg("Invalid token account")]
    InvalidTokenAccount,
    #[msg("No tokens")]
    NoTokens,
    #[msg("Error transfering tokens")]
    TransferError,
    #[msg("Already initialized")]
    AlreadyInitialized,
    #[msg("Nothing to Claim")]
    NothingToClaim,
    #[msg("Error while trying to mint tokens")]
    MintError,
    #[msg("Is not owner")]
    NotOwner,
    #[msg("Inactive pool")]
    InactivePool,
}
