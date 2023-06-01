use crate::Driver;
use argh::FromArgs;

#[derive(FromArgs)]
/// A generic compiler driver.
struct FakeArgs {
    /// the input file
    #[argh(positional)]
    input: String,

    /// the output file
    #[argh(option, short = 'o')]
    output: Option<String>,
}

pub fn cli(driver: &Driver) {
    let args: FakeArgs = argh::from_env();
}
