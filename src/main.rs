use std::env;
use std::fs;
use std::fs::File;
use std::process;

use crate::basic::engine::calculate_from_csv;
use crate::basic::util::init_logger;
pub mod basic;
pub mod sparse;
use sprs::{CsMat, CsVec};

fn main() {
    let args: Vec<String> = env::args().collect();
    init_logger();

    if args.len() < 3 {
        log::error!(
            "Usage: {} <localtrust_csv_path> <pretrust_csv_path>",
            args[0]
        );
        process::exit(1);
    }

    let localtrust_csv_path = &args[1];
    let pretrust_csv_path = &args[2];

    let localtrust_csv =
        fs::read_to_string(localtrust_csv_path).expect("Failed to read localtrust CSV file");
    let pretrust_csv =
        fs::read_to_string(pretrust_csv_path).expect("Failed to read pretrust CSV file");

    let (result, result_old) = calculate_from_csv(&localtrust_csv, &pretrust_csv, None).unwrap();

    // println!("{:?}", result);

    let num_records = 20;

    println!("{:?} {:?}", "result", "result_old");
    println!("===========================");

    // Compare the first 10 records of both vectors
    for i in 0..num_records {
        println!("{:?} {:<?}", result[i], result_old[i]);
    }
}
