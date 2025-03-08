An example for market making with DLMM

### Build 
```
cargo build
```

### Run
target/debug/market_making --help


### Check positions:
`http://localhost:8080/check_positions`

### run
```
cargo run --bin market_making -- --provider https://mainnet.helius-rpc.com/?api-key=f17bedeb-4c13-407b-9b80-f4a6f4599fe3 --wallet ./src/WALLET.json --config ./src/config.json
```
