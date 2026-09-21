#[cfg(feature = "experimental-burn-net8-packed-cuda-v1")]
fn main() {
    let result=(|| -> Result<(),String> {
        let path=std::env::args().nth(1).ok_or("usage: public_terminal_fixture_v1 COMMAND.json")?;
        let command=serde_json::from_slice(&std::fs::read(path).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        let result=mtg_kernel::expanded_deck_training_v1::stack_features::terminal_tactics::fixtures::run(command)?;
        println!("{result}");Ok(())
    })();
    if let Err(error)=result {eprintln!("{error}");std::process::exit(1);}
}
#[cfg(not(feature = "experimental-burn-net8-packed-cuda-v1"))]
fn main() {eprintln!("requires experimental-burn-net8-packed-cuda-v1");std::process::exit(1);}
