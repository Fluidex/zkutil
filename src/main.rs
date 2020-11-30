extern crate bellman_ce;
extern crate clap;
extern crate zkutil;

use bellman_ce::pairing::bn256::Bn256;
use clap::Clap;
use std::fs;
use std::fs::File;
use std::path::Path;
use std::str;
use std::time::Instant;
use zkutil::circom_circuit::{
    create_rng, create_verifier_sol_file, generate_random_parameters, groth16_verify, load_inputs_json_file, load_params_file,
    load_proof_json_file, plonk_verify, proof_to_json_file, prove as prove2, proving_key_json_file, r1cs_from_bin_file,
    r1cs_from_json_file, verification_key_json_file, witness_from_json_file, CircomCircuit, R1CS,
};
use zkutil::io;
use zkutil::proofsys_type::ProofSystem;
use zkutil::prover;

/// A tool to work with SNARK circuits generated by circom
#[derive(Clap)]
struct Opts {
    #[clap(subcommand)]
    command: SubCommand,
}

#[derive(Clap)]
enum SubCommand {
    /// Generate a SNARK proof
    Prove(ProveOpts),
    /// Verify a SNARK proof
    Verify(VerifyOpts),
    /// Generate trusted setup parameters
    Setup(SetupOpts),
    /// Generate verifier smart contract
    GenerateVerifier(GenerateVerifierOpts),
    /// Export proving and verifying keys compatible with snarkjs/websnark
    ExportKeys(ExportKeysOpts),
}

/// A subcommand for generating a SNARK proof
#[derive(Clap)]
struct ProveOpts {
    /// Plonk universal setup power_of_two
    #[clap(short = "o", long = "power_of_two", default_value = "20")]
    power_of_two: u32,
    /// Plonk universal setup srs file in monomial form
    #[clap(short = "m", long = "srs_monomial_form")]
    srs_monomial_form: String,
    /// Plonk universal setup srs file in lagrange form
    #[clap(short = "l", long = "srs_lagrange_form")]
    srs_lagrange_form: Option<String>,
    /// Circuit R1CS or JSON file [default: circuit.r1cs|circuit.json]
    #[clap(short = "c", long = "circuit")]
    circuit: Option<String>,
    /// Witness JSON file
    #[clap(short = "w", long = "witness", default_value = "witness.json")]
    witness: String,
    /// Output file for proof BIN
    #[clap(short = "p", long = "proof", default_value = "proof.bin")]
    proof: String,

    // TODO:
    // /// Output file for public inputs JSON
    // #[clap(short = "o", long = "public", default_value = "public.json")]
    // public: String,
    /// Proof system
    #[clap(short = "s", long = "proof_system", default_value = "groth16")]
    proof_system: ProofSystem,
}

/// A subcommand for verifying a SNARK proof
#[derive(Clap)]
struct VerifyOpts {
    /// Proof BIN file
    #[clap(short = "p", long = "proof", default_value = "proof.bin")]
    proof: String,
    /// Verification key file
    #[clap(short = "v", long = "verification_key", default_value = "vk.bin")]
    vk: String,
    /// Proof system
    #[clap(short = "s", long = "proof_system", default_value = "plonk")]
    proof_system: ProofSystem,
}

/// A subcommand for generating a trusted setup parameters
#[derive(Clap)]
struct SetupOpts {
    /// Snark trusted setup parameters file
    #[clap(short = "p", long = "params", default_value = "params.bin")]
    params: String,
    /// Circuit R1CS or JSON file [default: circuit.r1cs|circuit.json]
    #[clap(short = "c", long = "circuit")]
    circuit: Option<String>,
    /// Proof system
    #[clap(short = "s", long = "proof_system", default_value = "groth16")]
    proof_system: ProofSystem,
}

/// A subcommand for generating a Solidity verifier smart contract
#[derive(Clap)]
struct GenerateVerifierOpts {
    /// Snark trusted setup parameters file
    #[clap(short = "p", long = "params", default_value = "params.bin")]
    params: String,
    /// Output smart contract name
    #[clap(short = "v", long = "verifier", default_value = "Verifier.sol")]
    verifier: String,
    /// Proof system
    #[clap(short = "s", long = "proof_system", default_value = "groth16")]
    proof_system: ProofSystem,
}

/// A subcommand for exporting proving and verifying keys compatible with snarkjs/websnark
#[derive(Clap)]
struct ExportKeysOpts {
    /// Snark trusted setup parameters file
    #[clap(short = "p", long = "params", default_value = "params.bin")]
    params: String,
    /// Circuit R1CS or JSON file [default: circuit.r1cs|circuit.json]
    #[clap(short = "c", long = "circuit")]
    circuit: Option<String>,
    /// Output proving key file
    #[clap(short = "r", long = "pk", default_value = "proving_key.json")]
    pk: String,
    /// Output verifying key file
    #[clap(short = "v", long = "vk", default_value = "verification_key.json")]
    vk: String,
    /// Proof system
    #[clap(short = "s", long = "proof_system", default_value = "groth16")]
    proof_system: ProofSystem,
}

fn main() {
    let opts: Opts = Opts::parse();
    match opts.command {
        SubCommand::Prove(o) => {
            println!("Running with proof system: {:?}", o.proof_system);
            prove(o);
        }
        SubCommand::Verify(o) => {
            println!("Running with proof system: {:?}", o.proof_system);
            verify(o);
        }
        SubCommand::Setup(o) => {
            println!("Running with proof system: {:?}", o.proof_system);
            setup(o);
        }
        SubCommand::GenerateVerifier(o) => {
            println!("Running with proof system: {:?}", o.proof_system);
            generate_verifier(o);
        }
        SubCommand::ExportKeys(o) => {
            println!("Running with proof system: {:?}", o.proof_system);
            export_keys(o);
        }
    }
}

fn load_r1cs(filename: &str) -> R1CS<Bn256> {
    if filename.ends_with("json") {
        r1cs_from_json_file(filename)
    } else {
        let (r1cs, _wire_mapping) = r1cs_from_bin_file(filename).unwrap();
        r1cs
    }
}

fn resolve_circuit_file(filename: Option<String>) -> String {
    match filename {
        Some(s) => s,
        None => {
            if Path::new("circuit.r1cs").exists() || !Path::new("circuit.json").exists() {
                "circuit.r1cs".to_string()
            } else {
                "circuit.json".to_string()
            }
        }
    }
}

fn prove(opts: ProveOpts) {
    assert!(opts.proof_system == ProofSystem::Plonk, "Deprecated");
    // TODO: refactor
    assert!(
        (prover::SETUP_MIN_POW2..=prover::SETUP_MAX_POW2).contains(&opts.power_of_two),
        "setup power of two is not in the correct range"
    );

    let circuit_file = resolve_circuit_file(opts.circuit);
    println!("Loading circuit from {}...", circuit_file);
    let circuit = CircomCircuit {
        r1cs: load_r1cs(&circuit_file),
        witness: Some(witness_from_json_file::<Bn256>(&opts.witness)),
        wire_mapping: None,
        aux_offset: opts.proof_system.aux_offset(),
    };

    let setup = prover::SetupForProver::prepare_setup_for_prover(circuit).expect("prepare err");

    let timer = Instant::now();
    // let proof = prove_by_steps::<_, _, RollingKeccakTranscript<<E as ScalarEngine>::Fr>>(
    //         circuit,
    //         &self.hints,
    //         &self.setup_polynomials,
    //         None,
    //         self.key_monomial_form.as_ref().expect("Setup should have universal setup struct"),
    //     )?;
    log::info!("Proving takes {:?}", timer.elapsed());
    let writer = File::create(&opts.proof).unwrap();
    // proof.write(writer).unwrap();
    println!("Proof saved to {}", opts.proof);
}

fn verify(opts: VerifyOpts) {
    assert!(opts.proof_system == ProofSystem::Plonk, "Deprecated");

    let vk = io::load_verification_key::<Bn256>(&opts.vk);
    let proof = io::load_proof::<Bn256>(&opts.proof);
    let correct = plonk_verify(&vk, &proof).unwrap();
    if correct {
        println!("Proof is correct");
    } else {
        println!("Proof is invalid!");
        std::process::exit(400);
    }
}

fn setup(opts: SetupOpts) {
    if opts.proof_system == ProofSystem::Plonk {
        unimplemented!();
    }

    let circuit_file = resolve_circuit_file(opts.circuit);
    println!("Loading circuit from {}...", circuit_file);
    let rng = create_rng();
    let circuit = CircomCircuit {
        r1cs: load_r1cs(&circuit_file),
        witness: None,
        wire_mapping: None,
        aux_offset: opts.proof_system.aux_offset(),
    };
    println!("Generating trusted setup parameters...");
    let params = generate_random_parameters(circuit, rng).unwrap();
    println!("Writing to file...");
    let writer = File::create(&opts.params).unwrap();
    params.write(writer).unwrap();
    println!("Saved parameters to {}", opts.params);
}

fn generate_verifier(opts: GenerateVerifierOpts) {
    if opts.proof_system == ProofSystem::Plonk {
        unimplemented!();
    }

    let params = load_params_file(&opts.params);
    create_verifier_sol_file(&params, &opts.verifier).unwrap();
    println!("Created {}", opts.verifier);
}

fn export_keys(opts: ExportKeysOpts) {
    if opts.proof_system == ProofSystem::Plonk {
        unimplemented!();
    }

    println!("Exporting {}...", opts.params);
    let params = load_params_file(&opts.params);
    let circuit_file = resolve_circuit_file(opts.circuit);
    let circuit = CircomCircuit {
        r1cs: load_r1cs(&circuit_file),
        witness: None,
        wire_mapping: None,
        aux_offset: opts.proof_system.aux_offset(),
    };
    proving_key_json_file(&params, circuit, &opts.pk).unwrap();
    verification_key_json_file(&params, &opts.vk).unwrap();
    println!("Created {} and {}.", opts.pk, opts.vk);
}
