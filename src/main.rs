use std::{
    collections::HashMap,
    fs,
    path::PathBuf,
    str::FromStr,
    sync::Arc,
    time::{Duration, Instant},
};

use clap::{Parser, Subcommand};
use eyre::{bail, ContextCompat};
use ore::{
    state::{Bus, Proof, Treasury},
    utils::AccountDeserialize,
};
use serde_json::json;
use solana_client::{
    nonblocking::rpc_client::RpcClient,
    rpc_request::RpcRequest,
    rpc_response::{Response, RpcBlockhash},
};
use solana_sdk::{
    account::{Account, ReadableAccount},
    clock::{Clock, Slot},
    commitment_config::CommitmentConfig,
    keccak::Hash,
    pubkey::Pubkey,
    signature::{Keypair, Signature},
    signer::EncodableKey,
    sysvar,
};
use solana_transaction_status::TransactionStatus;
use tokio::io::AsyncWriteExt;
use tracing::{error, log};

mod constant;
mod jito;
mod utils;
mod bundle_transfer;

#[tokio::main(flavor = "multi_thread")]
async fn main() {
    Miner::init_pretty_env_logger();
    let miner = Miner::parse();

    match &miner.command {
        Command::JitoTipStream => miner.jito_tip_stream().await,
        Command::BundleTransfer(args) => miner.bundle_transfer(args).await,
    }
}

#[derive(Parser, Debug, Clone)]
pub struct Miner {
    #[arg(long, default_value = "https://api.mainnet-beta.solana.com")]
    pub rpc: String,

    #[arg(long)]
    pub priority_fee: Option<u64>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Command {
    JitoTipStream,
    BundleTransfer(crate::bundle_transfer::BatchTransferArgs),
}

impl Miner {
    pub fn init_pretty_env_logger() {
        env_logger::Builder::new()
            .filter_level(log::LevelFilter::Info)
            .parse_default_env()
            .init();
    }

    pub fn get_client_confirmed(rpc: &str) -> Arc<RpcClient> {
        Arc::new(RpcClient::new_with_commitment(
            rpc.to_string(),
            CommitmentConfig::confirmed(),
        ))
    }

    pub fn read_keys(key_folder: &str) -> Vec<Keypair> {
        fs::read_dir(key_folder)
            .expect("Failed to read key folder")
            .map(|entry| {
                let path = entry.expect("Failed to read entry").path();

                Keypair::read_from_file(&path).unwrap_or_else(|_| panic!("Failed to read keypair from {:?}", path))
            })
            .collect::<Vec<_>>()
    }

    pub async fn get_latest_blockhash_and_slot(client: &RpcClient) -> eyre::Result<(Slot, solana_sdk::hash::Hash)> {
        let (blockhash, send_at_slot) = match client
            .send::<Response<RpcBlockhash>>(RpcRequest::GetLatestBlockhash, json!([{"commitment": "confirmed"}]))
            .await
        {
            Ok(r) => (r.value.blockhash, r.context.slot),
            Err(err) => eyre::bail!("failed to get latest blockhash: {err:#}"),
        };

        let blockhash = match solana_sdk::hash::Hash::from_str(&blockhash) {
            Ok(b) => b,
            Err(err) => eyre::bail!("fail to parse blockhash: {err:#}"),
        };

        Ok((send_at_slot, blockhash))
    }

    pub async fn get_balances(client: &RpcClient, accounts: &[Pubkey]) -> eyre::Result<HashMap<Pubkey, u64>> {
        let account_data = match client.get_multiple_accounts(accounts).await {
            Ok(a) => a,
            Err(err) => eyre::bail!("fail to get accounts: {err:#}"),
        };

        let result = account_data
            .into_iter()
            .zip(accounts.iter())
            .filter(|(account, _)| account.is_some())
            .map(|(account, pubkey)| (*pubkey, account.unwrap().lamports))
            .collect();

        Ok(result)
    }

    pub async fn get_signature_statuses(
        client: &RpcClient,
        signatures: &[Signature],
    ) -> eyre::Result<(Vec<Option<TransactionStatus>>, Slot)> {
        let signatures_params = signatures.iter().map(|s| s.to_string()).collect::<Vec<_>>();

        let (statuses, slot) = match client
            .send::<Response<Vec<Option<TransactionStatus>>>>(
                RpcRequest::GetSignatureStatuses,
                json!([signatures_params]),
            )
            .await
        {
            Ok(result) => (result.value, result.context.slot),
            Err(err) => eyre::bail!("fail to get bundle status: {err}"),
        };

        Ok((statuses, slot))
    }
}
