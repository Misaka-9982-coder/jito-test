use std::{collections::HashMap};

use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey, signature::Signature};
use solana_transaction_status::TransactionStatus;

pub fn find_landed_txs(signatures: &[Signature], statuses: Vec<Option<TransactionStatus>>) -> Vec<Signature> {
    let landed_tx = statuses
        .into_iter()
        .zip(signatures.iter())
        .filter_map(|(status, sig)| {
            if status?.satisfies_commitment(CommitmentConfig::confirmed()) {
                Some(*sig)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    landed_tx
}

pub fn pick_richest_account(account_balances: &HashMap<Pubkey, u64>, accounts: &[Pubkey]) -> Pubkey {
    *accounts
        .iter()
        .max_by_key(|pubkey| account_balances.get(pubkey).unwrap())
        .expect("accounts should not be empty")
}

#[macro_export]
macro_rules! format_duration {
    ($d: expr) => {
        format_args!("{:.1}s", $d.as_secs_f64())
    };
}

#[macro_export]
macro_rules! format_reward {
    ($r: expr) => {
        format_args!("{:.}", utils::ore_ui_amount($r))
    };
}

#[macro_export]
macro_rules! wait_return {
    ($duration: expr) => {{
        tokio::time::sleep(std::time::Duration::from_millis($duration)).await;
        return;
    }};

    ($duration: expr, $return: expr) => {{
        tokio::time::sleep(std::time::Duration::from_millis($duration)).await;
        return $return;
    }};
}

#[macro_export]
macro_rules! wait_continue {
    ($duration: expr) => {{
        tokio::time::sleep(std::time::Duration::from_millis($duration)).await;
        continue;
    }};
}
