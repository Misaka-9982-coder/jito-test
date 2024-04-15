use solana_sdk::{commitment_config::CommitmentConfig, signature::Signature};
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

