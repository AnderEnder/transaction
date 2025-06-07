use std::fs::File;
use std::iter::Iterator;
use std::{env, io::BufReader};

use transaction::payments_engine::PaymentEngine;
use transaction::processor::process_csv_stream;

fn main() {
    let mut args = env::args();
    if args.len() != 2 {
        eprintln!("Usage: {} transactions.csv", args.next().unwrap());
        return;
    }

    let filename = args.nth(1).expect("No filename provided");
    let mut engine = PaymentEngine::new();

    let reader = BufReader::new(File::open(&filename).expect("Failed to open file"));
    process_csv_stream(&mut engine, reader);

    println!("{}", engine);
}
