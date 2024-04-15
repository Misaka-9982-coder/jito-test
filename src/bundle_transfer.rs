use std::{collections::HashSet, time::Duration};

use clap::Parser;
use rand::Rng;
use solana_sdk::{
    instruction::Instruction, pubkey::Pubkey, signer::Signer, system_instruction, transaction::Transaction,
};
use tracing::{error, info};

use crate::{constant, jito, utils, Miner};

#[derive(Parser, Debug, Clone)]
pub struct BatchTransferArgs {
    #[arg(long, help = "The folder that contains all the keys used for transfer")]
    pub key_folder: String,

    #[arg(long, help = "The recipient address to receive SOL")]
    pub recipient: Pubkey,

    #[arg(long, help = "The amount of SOL to transfer")]
    pub amount: u64,
}

impl Miner {
    pub async fn bundle_transfer(&self, args: &BatchTransferArgs) {
        let client = Miner::get_client_confirmed(&self.rpc);
        let accounts = Self::read_keys(&args.key_folder);
        let jito_tip = self.priority_fee.expect("jito tip is required");

        let mut registered = HashSet::new();

        for batch in accounts.chunks(constant::FETCH_ACCOUNT_LIMIT) {
            let pubkeys: Vec<_> = batch.iter().map(|a| a.pubkey()).collect();
            client
                .get_multiple_accounts(&pubkeys)
                .await
                .expect("Failed to get accounts")
                .into_iter()
                .zip(batch.iter())
                .for_each(|(account, signer)| {
                    if account.is_some() {
                        registered.insert(signer.pubkey());
                    }
                });
        }

        let accounts = accounts
            .into_iter()
            .filter(|signer| registered.contains(&signer.pubkey()))
            .collect::<Vec<_>>();

        info!("transferring {} accounts", accounts.len());

        let mut batch_iter = accounts.chunks(5);
        let mut remaining = accounts.len();

        let mut txs = vec![];
        let mut accounts_in_this_batch = 0;
        let mut signers_for_txs = vec![];

        loop {
            while txs.len() < 5 {
                let batch = match batch_iter.next() {
                    Some(batch) => batch,
                    None => break,
                };

                let mut ixs = vec![];
                let mut signers = vec![];

                for signer in batch {
                    ixs.push(system_instruction::transfer(
                        &signer.pubkey(),
                        &args.recipient,
                        args.amount,
                    ));
                    signers.push(signer);
                }

                let fee_payer = signers[rand::thread_rng().gen_range(0..signers.len())].pubkey();

                if txs.is_empty() {
                    ixs.push(jito::build_bribe_ix(&fee_payer, jito_tip));
                }

                txs.push(Transaction::new_with_payer(&ixs, Some(&fee_payer)));
                accounts_in_this_batch += signers.len();
                signers_for_txs.push(signers);
            }


            if txs.is_empty() {
                break;
            }

            let (send_at_slot, blockhash) = match Self::get_latest_blockhash_and_slot(&client).await {
                Ok(value) => value,
                Err(err) => {
                    error!("fail to get latest blockhash: {err:#}");
                    continue;
                }
            };

            let bundle = txs
                .iter()
                .zip(signers_for_txs.iter())
                .map(|(tx, signers)| {
                    let mut tx = tx.clone();
                    tx.sign(signers.as_slice(), blockhash);
                    tx
                })
                .collect::<Vec<_>>();

            let mut failed_batch = false;

            for tx in &bundle {
                let sim_result = match client.simulate_transaction(tx).await {
                    Ok(r) => r.value,
                    Err(err) => {
                        error!("fail to simulate transaction: {err:#}");
                        failed_batch = true;
                        break;
                    }
                };

                if let Some(err) = sim_result.err {
                    error!("fail to simulate transaction: {err:#}");
                    failed_batch = true;
                    break;
                }
            }

            if failed_batch {
                txs.clear();
                remaining -= accounts_in_this_batch;
                signers_for_txs.clear();
                accounts_in_this_batch = 0;
                continue;
            }

            let (tx, bundle_id) = jito::send_bundle(bundle).await.unwrap();

            info!(first_tx = ?tx, %bundle_id, accounts = accounts_in_this_batch, remaining, slot = send_at_slot, "bundle sent");

            let mut latest_slot = send_at_slot;
            let mut mined = false;

            while !mined && latest_slot < send_at_slot + constant::SLOT_EXPIRATION {
                tokio::time::sleep(Duration::from_secs(2)).await;

                let (statuses, slot) = match Self::get_signature_statuses(&client, &[tx]).await {
                    Ok(value) => value,
                    Err(err) => {
                        error!(send_at_slot, "fail to get bundle status: {err:#}");
                        tokio::time::sleep(Duration::from_secs(2)).await;
                        continue;
                    }
                };

                mined = !utils::find_landed_txs(&[tx], statuses).is_empty();
                latest_slot = slot;
            }

            if mined {
                txs.clear();
                remaining -= accounts_in_this_batch;
                signers_for_txs.clear();
                accounts_in_this_batch = 0;
                info!(
                    accounts = accounts_in_this_batch,
                    remaining, "bundle sent at slot {send_at_slot}, remaining accounts: {remaining}"
                );
            } else {
                error!(accounts = accounts_in_this_batch, remaining, "bundle dropped, retrying");
            }
        
        }
    }
}