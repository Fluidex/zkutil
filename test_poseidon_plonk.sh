#!/bin/bash
set -ex
DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" >/dev/null 2>&1 && pwd )"
TOOL_DIR=$DIR"/contrib"
CIRCUIT_DIR=$DIR"/testdata/poseidon"

# from zksync/infrastructure/zk/src/run/run.ts
echo "Step1: download universal setup file"
pushd keys/setup
axel -ac https://universal-setup.ams3.digitaloceanspaces.com/setup_2^20.key || true
popd

echo "Step2: compile circuit and calculate witness using snarkjs"
. $TOOL_DIR/process_circom_circuit.sh

echo "Step3: test prove and verify" 
RUST_LOG=info cargo test --release simple_plonk_test

echo "Step4: verify" 
cargo run --release verify -s plonk -p $CIRCUIT_DIR/proof.bin -v $CIRCUIT_DIR/vk.bin
