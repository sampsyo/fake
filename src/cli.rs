use crate::{Driver, Emitter, Request, State};
use argh::FromArgs;
use std::fmt::Display;
use std::path::PathBuf;
use std::str::FromStr;

enum Mode {
    Emit,
    Plan,
}

impl FromStr for Mode {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "emit" => Ok(Mode::Emit),
            "plan" => Ok(Mode::Plan),
            _ => Err("unknown mode".to_string()),
        }
    }
}

impl Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mode::Emit => write!(f, "emit"),
            Mode::Plan => write!(f, "plan"),
        }
    }
}

#[derive(FromArgs)]
/// A generic compiler driver.
struct FakeArgs {
    /// the input file
    #[argh(positional)]
    input: PathBuf,

    /// the output file
    #[argh(option, short = 'o')]
    output: Option<PathBuf>,

    /// the state to start from
    #[argh(option)]
    from: Option<String>,

    /// the state to produce
    #[argh(option)]
    to: Option<String>,

    /// execution mode (plan, emit)
    #[argh(option, default = "Mode::Emit")]
    mode: Mode,
}

type Result<T> = std::result::Result<T, &'static str>;

fn from_state(driver: &Driver, args: &FakeArgs) -> Result<State> {
    match &args.from {
        Some(name) => driver.get_state(name).ok_or("unknown --from state"),
        None => driver
            .guess_state(&args.input)
            .ok_or("could not infer input state"),
    }
}

fn to_state(driver: &Driver, args: &FakeArgs) -> Result<State> {
    match &args.to {
        Some(name) => driver.get_state(name).ok_or("unknown --to state"),
        None => match &args.output {
            Some(out) => driver
                .guess_state(out)
                .ok_or("could not infer output state"),
            None => Err("specify an output file or use --to"),
        },
    }
}

fn get_request(driver: &Driver, args: &FakeArgs) -> Result<Request> {
    Ok(Request {
        input: from_state(driver, args)?,
        output: to_state(driver, args)?,
    })
}

pub fn cli(driver: &Driver) {
    let args: FakeArgs = argh::from_env();

    let req = get_request(driver, &args).unwrap_or_else(|e| {
        eprintln!("error: {}", e);
        std::process::exit(1);
    });

    let plan = driver.plan(req.input, req.output).unwrap_or_else(|| {
        eprintln!("error: could not find path");
        std::process::exit(1);
    });

    match args.mode {
        Mode::Plan => {
            for step in &plan.steps {
                println!("{}: {}", step, driver.ops[*step].name);
            }
        }
        Mode::Emit => {
            let mut emitter = Emitter::default();
            emitter.emit(&driver, plan, &args.input, args.output.as_deref());
        } // TODO a future "run" mode
    }
}
