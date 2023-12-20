use crate::strategy_handler::base::StrategyHandler;
use crate::strategy_handler::port_adapter::PortReserve;
use crate::user::get_or_create_ata;
use crate::utils::get_program;
use anchor_client::Cluster;
use anchor_lang::InstructionData;
use anchor_lang::ToAccountMetas;
use anyhow::Result;
use async_trait::async_trait;
use mercurial_vault::strategy::base::get_port_finance_program_id;
use solana_program::pubkey::Pubkey;
use solana_program::sysvar;
use solana_sdk::instruction::AccountMeta;
use solana_sdk::instruction::Instruction;
use solana_sdk::signer::keypair::Keypair;
use solana_sdk::signer::Signer;
use std::ops::Deref;
use std::str::FromStr;
pub struct PortFinanceWithoutLMHandler {}

#[async_trait]
impl StrategyHandler for PortFinanceWithoutLMHandler {
    async fn withdraw_directly_from_strategy(
        &self,
        url: Cluster,
        // payer: &Keypair,
        strategy: Pubkey,
        token_mint: Pubkey,
        base: Pubkey,
        partner: String,
        amount: u64,
    ) -> Result<()> {
        let program_client = get_program(url)?;
        let (vault, _vault_bump) = Pubkey::find_program_address(
            &[
                mercurial_vault::seed::VAULT_PREFIX.as_ref(),
                token_mint.as_ref(),
                base.as_ref(),
            ],
            &mercurial_vault::id(),
        );

        let vault_state: mercurial_vault::state::Vault = program_client.account(vault).await?;
        let strategy_state: mercurial_vault::state::Strategy =
            program_client.account(strategy).await?;

        let reserve_state: PortReserve = program_client.account(strategy_state.reserve).await?;

        let collateral_mint = reserve_state.collateral.mint_pubkey;

        let (collateral_vault, _collateral_vault_bump) = Pubkey::find_program_address(
            &[
                mercurial_vault::seed::COLLATERAL_VAULT_PREFIX.as_ref(),
                strategy.as_ref(),
            ],
            &mercurial_vault::id(),
        );

        let (lending_market_authority, _bump_seed) = Pubkey::find_program_address(
            &[&reserve_state.lending_market.as_ref()],
            &get_port_finance_program_id(),
        );
        let user_token =
            get_or_create_ata(&program_client, token_mint, program_client.payer()).await?;

        let partner = Pubkey::from_str(&partner).unwrap();
        let partner_token = get_or_create_ata(&program_client, token_mint, partner).await?;
        let (partner, _nonce) = Pubkey::find_program_address(
            &[vault.as_ref(), partner_token.as_ref()],
            &affiliate::id(),
        );
        // check whether partner is existed
        let _partner_state: affiliate::Partner = program_client.account(partner).await?;
        let (user, _nonce) = Pubkey::find_program_address(
            &[partner.as_ref(), program_client.payer().as_ref()],
            &affiliate::id(),
        );
        // check whether user is existed
        let _user_state: affiliate::User = program_client.account(user).await?;
        let user_lp = get_or_create_ata(&program_client, vault_state.lp_mint, user).await?;
        let mut accounts = affiliate::accounts::WithdrawDirectlyFromStrategy {
            partner,
            user,
            vault,
            vault_program: mercurial_vault::id(),
            strategy,
            reserve: strategy_state.reserve,
            strategy_program: get_port_finance_program_id(),
            collateral_vault,
            token_vault: vault_state.token_vault,
            fee_vault: vault_state.fee_vault,
            vault_lp_mint: vault_state.lp_mint,
            user_token,
            user_lp,
            owner: program_client.payer(),
            token_program: spl_token::id(),
        }
        .to_account_metas(None);
        let mut remaining_accounts = vec![
            AccountMeta::new(reserve_state.liquidity.supply_pubkey, false),
            AccountMeta::new_readonly(reserve_state.lending_market, false),
            AccountMeta::new_readonly(lending_market_authority, false),
            AccountMeta::new(collateral_mint, false),
            AccountMeta::new_readonly(sysvar::clock::id(), false),
        ];

        accounts.append(&mut remaining_accounts);

        let instructions = vec![
            port_variable_rate_lending_instructions::instruction::refresh_reserve(
                get_port_finance_program_id(),
                strategy_state.reserve,
                reserve_state.liquidity.oracle_pubkey,
            ),
            Instruction {
                program_id: affiliate::id(),
                accounts,
                data: affiliate::instruction::WithdrawDirectlyFromStrategy {
                    unmint_amount: amount,
                    min_out_amount: 0,
                }
                .data(),
            },
        ];

        let builder = program_client.request();
        let builder = instructions
            .into_iter()
            .fold(builder, |bld, ix| bld.instruction(ix));

        let signature = builder.send().await?;
        println!("{}", signature);
        Ok(())
    }
}
