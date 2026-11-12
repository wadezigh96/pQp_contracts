use std::{env, fs, process};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 5 || args[1] != "verify" {
        eprintln!("usage: qpc-near-proof verify <trusted-head-block-merkle-root> <qpc-escrow-account> <proof.json>");
        process::exit(2);
    }
    let proof = fs::read_to_string(&args[4]).unwrap_or_else(|e| {
        eprintln!("cannot read proof file: {e}");
        process::exit(2);
    });
    match qpc_near_proof::verify_qpc_payment_proof(&proof, &args[2], &args[3]) {
        Ok(payment) => {
            println!("verified QPC payment inclusion:");
            println!("payment_id={}", payment.payment_id);
            println!("token_id={}", payment.token_id);
            println!("payer={}", payment.payer);
            println!("recipient={}", payment.recipient);
            println!("amount={}", payment.amount);
            println!("outcome_id={}", payment.outcome_id);
            println!("block_hash={}", payment.block_hash);
            println!("NOTE: trusted root authentication is external; this is not a NEAR consensus light client.");
        }
        Err(error) => {
            eprintln!("verification failed: {error}");
            process::exit(1);
        }
    }
}
