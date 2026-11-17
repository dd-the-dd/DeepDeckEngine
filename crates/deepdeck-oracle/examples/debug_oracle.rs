//! Small native-parser entry point for stepping through an editable request.
//! No expected-outcome fixture and no engine initialization are required.
use deepdeck_oracle::oracle::{OracleCardParseRequest, parse_oracle_card_native};
use std::{error::Error, fs, io};

fn main() -> Result<(), Box<dyn Error>> {
    let input = std::env::args_os()
        .nth(1)
        .ok_or_else(|| io::Error::other("usage: debug_oracle REQUEST.json"))?;
    let request: OracleCardParseRequest = serde_json::from_slice(&fs::read(input)?)?;
    let result = std::thread::Builder::new()
        .name("oracle-debug".to_string())
        .stack_size(64 * 1024 * 1024)
        .spawn(move || parse_oracle_card_native(request))?
        .join()
        .map_err(|_| io::Error::other("native parser thread panicked"))?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}
