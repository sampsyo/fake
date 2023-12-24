use crate::{Driver, Emitter, Request, State};
use argh::FromArgs;
use std::fmt::Display;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;

enum Mode {
    EmitNinja,
    ShowPlan,
    Generate,
}
// TODO: Future modes: generate, run

impl FromStr for Mode {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "emit" => Ok(Mode::EmitNinja),
            "plan" => Ok(Mode::ShowPlan),
            "gen" => Ok(Mode::Generate),
            _ => Err("unknown mode".to_string()),
        }
    }
}

impl Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Mode::EmitNinja => write!(f, "emit"),
            Mode::ShowPlan => write!(f, "plan"),
            Mode::Generate => write!(f, "gen"),
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
    #[argh(option, default = "Mode::EmitNinja")]
    mode: Mode,

    /// working directory for the build
    #[argh(option)]
    dir: Option<PathBuf>,
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

fn get_request(driver: &Driver, args: &FakeArgs, workdir: &Path) -> Result<Request> {
    let in_path = relative_path(&args.input, workdir);
    let out_path = match &args.output {
        Some(out_path) => Some(relative_path(out_path, workdir)),
        None => None,
    };

    Ok(Request {
        start_file: in_path,
        start_state: from_state(driver, args)?,
        end_file: out_path,
        end_state: to_state(driver, args)?,
    })
}

/// Generate a path referring to the same file as `path` that is usable when the working directory
/// is `base`. This can always just be `path.canonical()` as a fallback, but sometimes we can
/// opportunistically make it a little friendlier.
fn relative_path(path: &Path, base: &Path) -> PathBuf {
    if base == Path::new(".") {
        path.to_path_buf()
    } else if path.is_absolute() {
        path.to_path_buf()
    } else if path.starts_with(base) {
        path.strip_prefix(base).unwrap().to_path_buf()
    } else if base.is_relative() {
        if base
            .components()
            .find(|c| c == &std::path::Component::ParentDir)
            .is_some()
        {
            // Too hard to handle base paths with `..`.
            path.canonicalize().unwrap()
        } else {
            // A special case when, e.g., base is just a subdirectory of cwd. We
            // can get "back" to the current directroy above base via `..`.
            let mut out = PathBuf::new();
            for _ in base.components() {
                out.push("..");
            }
            out.push(path);
            out
        }
    } else {
        path.canonicalize().unwrap()
    }
}

fn cli_inner(driver: &Driver) -> Result<()> {
    let args: FakeArgs = argh::from_env();

    // The default working directory (if not specified) depends on the mode.
    let workdir = args.dir.clone().unwrap_or_else(|| {
        PathBuf::from(match args.mode {
            Mode::Generate => ".fake",
            _ => ".",
        })
    });

    let req = get_request(driver, &args, &workdir)?;
    let plan = driver.plan(req).ok_or("could not find path")?;

    match args.mode {
        Mode::ShowPlan => {
            println!("start: {}", plan.start.display());
            for (op, file) in &plan.steps {
                println!("{}: {} -> {}", op, driver.ops[*op].name, file.display());
            }
        }
        Mode::EmitNinja => {
            let mut emitter = Emitter::new(Box::new(std::io::stdout()));
            emitter.emit(&driver, plan);
        }
        Mode::Generate => {
            std::fs::create_dir_all(&workdir).map_err(|_| "could not create working directory")?;

            let ninja_path = workdir.join("build.ninja");
            let ninja_file =
                std::fs::File::create(&ninja_path).map_err(|_| "could not create ninja file")?;
            let mut emitter = Emitter::new(Box::new(ninja_file));
            emitter.emit(&driver, plan);
        }
    }

    Ok(())
}

pub fn cli(driver: &Driver) {
    match cli_inner(driver) {
        Ok(()) => {}
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    }
}
