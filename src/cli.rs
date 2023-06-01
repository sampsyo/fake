use crate::Driver;
use argh::FromArgs;
use std::path::PathBuf;

#[derive(FromArgs)]
/// A generic compiler driver.
struct FakeArgs {
    /// the input file
    #[argh(positional)]
    input: PathBuf,

    /// the output file
    #[argh(option, short = 'o')]
    output: Option<PathBuf>,
}

pub fn cli(driver: &Driver) {
    let args: FakeArgs = argh::from_env();
    dbg!(driver.guess_state(&args.input));
}
