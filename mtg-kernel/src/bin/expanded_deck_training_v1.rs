mtg_kernel::fast_global_allocator!();

use mtg_kernel::expanded_deck_training_v1::{execute_v1, ExpandedTrainingCommandV1};

fn main() {
    mtg_kernel::opt_out_of_power_throttling_v1();
    // The command runs on a thread with the census binary's 256 MiB stack:
    // Windows gives the main thread 1 MiB, which the collection hooks' deep
    // play (W5/SD1 capture continuations) can overflow. Stack size changes no
    // computed byte.
    let result = std::thread::Builder::new()
        .name("expanded-deck-training".into())
        .stack_size(256 << 20)
        .spawn(run)
        .expect("spawn the command thread")
        .join();
    match result {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            eprintln!("expanded-deck training: {error}");
            std::process::exit(1);
        }
        Err(_) => {
            eprintln!("expanded-deck training: the command thread panicked");
            std::process::exit(101);
        }
    }
}
fn run() -> Result<(), String> {
    let mut args = std::env::args_os().skip(1);
    let path = args
        .next()
        .ok_or("usage: expanded_deck_training_v1 CONFIG.json")?;
    if args.next().is_some() {
        return Err("expected one config path".into());
    }
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("config exceeds 16 MiB".into());
    }
    let command: ExpandedTrainingCommandV1 =
        serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    let result = execute_v1(command)?;
    println!(
        "{}",
        serde_json::to_string(&result).map_err(|e| e.to_string())?
    );
    Ok(())
}
