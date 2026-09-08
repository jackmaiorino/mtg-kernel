use std::io::{self, Read};

fn main() {
    let mut bytes = Vec::new();
    if io::stdin().take(1_048_569).read_to_end(&mut bytes).is_err()
        || mtgo_blackbox_v1::parse_and_validate_visible_duel_producer_result_v1(&bytes).is_err()
    {
        std::process::exit(1);
    }
    println!("MTGO_VISIBLE_DUEL_PRODUCER_RESULT_V1:PASS");
}
