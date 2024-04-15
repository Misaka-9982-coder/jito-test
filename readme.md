https://gloriana-vjhz54-fast-mainnet.helius-rpc.com/

## Usage

### bundle transfer

```bash
cargo run --release -- \
    --rpc <RPC_URL> \
    --priority-fee 5000 \                      # Tip used for Jito bundle.
    bundle-transfer \
    --key-folder <FOLDER_CONTAINS_YOUR_KEYS> \ # Folder contains your Solana keys         
    --recipient <RECIPIENT_ADDRESS>  \         
    --amount <AMOUNT_TO_TRANSFER>
```