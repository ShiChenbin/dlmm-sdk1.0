use std::ops::Deref;

use anchor_client::solana_client::rpc_config::RpcSendTransactionConfig;
use anchor_client::{solana_sdk::pubkey::Pubkey, solana_sdk::signer::Signer, Program};

use anchor_lang::ToAccountMetas;
use anyhow::*;
use lb_clmm::accounts;
use lb_clmm::instruction;

pub async fn close_preset_parameter<C: Deref<Target = impl Signer> + Clone>(
    preset_parameter: Pubkey,
    program: &Program<C>,
    transaction_config: RpcSendTransactionConfig,
) -> Result<Pubkey> {
    let accounts = accounts::ClosePresetParameter {
        admin: program.payer(),
        rent_receiver: program.payer(),
        preset_parameter,
    }
    .to_account_metas(None);

    let ix = instruction::ClosePresetParameter {};

    let request_builder = program.request();
    let signature = request_builder
        .accounts(accounts)
        .args(ix)
        .send_with_spinner_and_config(transaction_config)
        .await;

    println!(
        "Close preset parameter {}. Signature: {signature:#?}",
        preset_parameter
    );

    signature?;

    Ok(preset_parameter)
}
