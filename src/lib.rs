use cranelift_entity::{entity_impl, PrimaryMap, SecondaryMap};
use std::collections::HashSet;
use std::ffi::OsStr;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

pub mod cli;
pub mod config;

/// The details about a given state.
pub struct State {
    pub name: String,
    pub extensions: Vec<String>,
}

/// A reference to a state.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct StateRef(u32);
entity_impl!(StateRef, "state");

impl State {
    /// Check whether a filename extension indicates this state.
    fn ext_matches(&self, ext: &str) -> bool {
        self.extensions.iter().any(|e| e == ext)
    }
}

/// A generated Ninja setup stanza.
/// TODO: Should these have, like, names and stuff?
pub trait Setup {
    fn setup(&self, emitter: &mut Emitter, run: &Run);
}

/// A reference to a setup.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct SetupRef(u32);
entity_impl!(SetupRef, "setup");

type EmitSetup = fn(&mut Emitter, &Run) -> ();

impl Setup for EmitSetup {
    fn setup(&self, emitter: &mut Emitter, run: &Run) {
        self(emitter, run)
    }
}

/// Metadata about an operation that controls when it applies.
struct OpMeta {
    pub name: String,
    pub input: StateRef,
    pub output: StateRef,
    pub setup: Option<SetupRef>,
}

/// The actual Ninja-generating machinery for an operation.
trait OpImpl {
    fn build(&self, emitter: &mut Emitter, input: &Path, output: &Path);
}

type EmitBuild = fn(&mut Emitter, &Path, &Path) -> ();

impl OpImpl for EmitBuild {
    fn build(&self, emitter: &mut Emitter, input: &Path, output: &Path) {
        (self)(emitter, input, output)
    }
}

/// An Operation transforms files from one State to another.
/// TODO: Someday, I would like to represent these as separate vectors (struct-of-arrays). This may
/// require switching from `cranelift-entity` to `id-arena`?
pub struct Operation {
    meta: OpMeta,
    impl_: Box<dyn OpImpl>,
}

/// An operation that works by applying a Ninja rule.
pub struct RuleOp {
    pub rule_name: String,
}

impl OpImpl for RuleOp {
    fn build(&self, emitter: &mut Emitter, input: &Path, output: &Path) {
        emitter.build(&self.rule_name, input, output);
    }
}

/// A reference to an operation.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct OpRef(u32);
entity_impl!(OpRef, "operation");

pub struct Driver {
    pub setups: PrimaryMap<SetupRef, Box<dyn Setup>>,
    pub states: PrimaryMap<StateRef, State>,
    pub ops: PrimaryMap<OpRef, Operation>,
}

impl Driver {
    pub fn find_path(&self, start: StateRef, end: StateRef) -> Option<Vec<OpRef>> {
        // Our start state is the input.
        let mut visited = SecondaryMap::<StateRef, bool>::new();
        visited[start] = true;

        // Build the incoming edges for each vertex.
        let mut breadcrumbs = SecondaryMap::<StateRef, Option<OpRef>>::new();

        // Breadth-first search.
        let mut state_queue: Vec<StateRef> = vec![start];
        while !state_queue.is_empty() {
            let cur_state = state_queue.remove(0);

            // Finish when we reach the goal.
            if cur_state == end {
                break;
            }

            // Traverse any edge from the current state to an unvisited state.
            for (op_ref, op) in self.ops.iter() {
                if op.meta.input == cur_state && !visited[op.meta.output] {
                    state_queue.push(op.meta.output);
                    visited[op.meta.output] = true;
                    breadcrumbs[op.meta.output] = Some(op_ref);
                }
            }
        }

        // Traverse the breadcrumbs backward to build up the path back from output to input.
        let mut op_path: Vec<OpRef> = vec![];
        let mut cur_state = end;
        while cur_state != start {
            match breadcrumbs[cur_state] {
                Some(op) => {
                    op_path.push(op);
                    cur_state = self.ops[op].meta.input;
                }
                None => return None,
            }
        }
        op_path.reverse();

        Some(op_path)
    }

    fn gen_name(&self, stem: &OsStr, op: OpRef) -> PathBuf {
        // Pick an appropriate extension for the output of this operation.
        let op = &self.ops[op];
        let ext = &self.states[op.meta.output].extensions[0];

        // TODO avoid collisions in case we reuse extensions...
        PathBuf::from(stem).with_extension(ext)
    }

    pub fn plan(&self, req: Request) -> Option<Plan> {
        // Find a path through the states.
        let path = self.find_path(req.start_state, req.end_state)?;

        // Generate filenames for each step.
        let stem = req.start_file.file_stem().expect("input filename missing");
        let mut steps: Vec<_> = path
            .into_iter()
            .map(|op| {
                let filename = self.gen_name(stem, op);
                (op, filename)
            })
            .collect();

        // If we have a specified output filename, use that instead of the generated one.
        // TODO this is ugly
        if let Some(end_file) = req.end_file {
            let last_step = steps.last_mut().expect("no steps");
            last_step.1 = end_file;
        }

        Some(Plan {
            start: req.start_file,
            steps,
        })
    }

    pub fn guess_state(&self, path: &Path) -> Option<StateRef> {
        let ext = path.extension()?.to_str()?;
        self.states
            .iter()
            .find(|(_, state_data)| state_data.ext_matches(ext))
            .map(|(state, _)| state)
    }

    pub fn get_state(&self, name: &str) -> Option<StateRef> {
        self.states
            .iter()
            .find(|(_, state_data)| state_data.name == name)
            .map(|(state, _)| state)
    }
}

#[derive(Default)]
pub struct DriverBuilder {
    setups: PrimaryMap<SetupRef, Box<dyn Setup>>,
    states: PrimaryMap<StateRef, State>,
    ops: PrimaryMap<OpRef, Operation>,
}

impl DriverBuilder {
    pub fn state(&mut self, name: &str, extensions: &[&str]) -> StateRef {
        self.states.push(State {
            name: name.to_string(),
            extensions: extensions.iter().map(|s| s.to_string()).collect(),
        })
    }

    fn add_op<T: OpImpl + 'static>(
        &mut self,
        name: &str,
        setup: Option<SetupRef>,
        input: StateRef,
        output: StateRef,
        impl_: T,
    ) -> OpRef {
        let meta = OpMeta {
            name: name.to_string(),
            setup,
            input,
            output,
        };
        self.ops.push(Operation {
            meta,
            impl_: Box::new(impl_),
        })
    }

    pub fn add_setup<T: Setup + 'static>(&mut self, setup: T) -> SetupRef {
        self.setups.push(Box::new(setup))
    }

    pub fn setup(&mut self, func: EmitSetup) -> SetupRef {
        self.add_setup(func)
    }

    pub fn op(
        &mut self,
        name: &str,
        setup: Option<SetupRef>,
        input: StateRef,
        output: StateRef,
        build: EmitBuild,
    ) -> OpRef {
        self.add_op(name, setup, input, output, build)
    }

    pub fn rule(
        &mut self,
        setup: Option<SetupRef>,
        input: StateRef,
        output: StateRef,
        rule_name: &str,
    ) -> OpRef {
        self.add_op(
            rule_name,
            setup,
            input,
            output,
            RuleOp {
                rule_name: rule_name.to_string(),
            },
        )
    }

    pub fn build(self) -> Driver {
        Driver {
            setups: self.setups,
            states: self.states,
            ops: self.ops,
        }
    }
}

#[derive(Debug)]
pub struct Request {
    pub start_state: StateRef,
    pub start_file: PathBuf,
    pub end_state: StateRef,
    pub end_file: Option<PathBuf>,
}

#[derive(Debug)]
pub struct Plan {
    pub start: PathBuf,
    pub steps: Vec<(OpRef, PathBuf)>,
}

pub struct Run<'a> {
    pub driver: &'a Driver,
    pub plan: Plan,
    pub config: config::Config,
}

impl<'a> Run<'a> {
    pub fn new(driver: &'a Driver, plan: Plan) -> Self {
        Self {
            driver,
            plan,
            config: config::Config::new().expect("failed to load config"),
        }
    }

    /// Just print the plan for debugging purposes.
    pub fn show(self) {
        println!("start: {}", self.plan.start.display());
        for (op, file) in &self.plan.steps {
            println!(
                "{}: {} -> {}",
                op,
                self.driver.ops[*op].meta.name,
                file.display()
            );
        }
    }

    /// Print the `build.ninja` file to stdout.
    pub fn emit_to_stdout(self) -> Result<(), std::io::Error> {
        let emitter = Emitter::new(std::io::stdout());
        emitter.emit(self)
    }

    /// Ensure that a directory exists and write `build.ninja` inside it.
    pub fn emit_to_dir(self, dir: &Path) -> Result<(), std::io::Error> {
        std::fs::create_dir_all(dir)?;
        let ninja_path = dir.join("build.ninja");
        let ninja_file = std::fs::File::create(ninja_path)?;

        let emitter = Emitter::new(ninja_file);
        emitter.emit(self)
    }

    /// Emit `build.ninja` to a temporary directory and then actually execute ninja.
    pub fn emit_and_run(self, dir: &Path) -> Result<(), std::io::Error> {
        // TODO: This workaround for lifetime stuff in the config isn't great.
        let keep = self.config.global.keep_build_dir;
        let ninja = self.config.global.ninja.clone();

        let stale_dir = dir.exists();
        self.emit_to_dir(dir)?;

        // Run `ninja` in the working directory.
        Command::new(ninja).current_dir(dir).status()?;

        // TODO consider printing final result to stdout, if it wasn't mapped to a file?
        // and also accepting input on stdin...

        // Remove the temporary directory unless it already existed at the start *or* the user specified `--keep`.
        if !keep && !stale_dir {
            std::fs::remove_dir_all(dir)?;
        }

        Ok(())
    }
}

pub struct Emitter {
    pub out: Box<dyn Write>,
}

impl Emitter {
    fn new<T: Write + 'static>(out: T) -> Self {
        Self { out: Box::new(out) }
    }

    fn emit(mut self, run: Run) -> Result<(), std::io::Error> {
        // Emit the setup for each operation used in the plan, only once.
        let mut done_setups = HashSet::<SetupRef>::new();
        for (op, _) in &run.plan.steps {
            if let Some(setup) = run.driver.ops[*op].meta.setup {
                if done_setups.insert(setup) {
                    writeln!(self.out, "# {}", setup).unwrap(); // TODO more descriptive name
                    run.driver.setups[setup].setup(&mut self, &run);
                    writeln!(self.out)?;
                }
            }
        }

        // Emit the build commands for each step in the plan.
        writeln!(self.out, "# build targets")?;
        let mut last_file = run.plan.start;
        for (op, out_file) in run.plan.steps {
            let op = &run.driver.ops[op];
            op.impl_.build(&mut self, &last_file, &out_file);
            last_file = out_file;
        }

        // Mark the last file as the default target.
        writeln!(self.out)?;
        write!(self.out, "default ")?;
        self.out
            .write_all(last_file.as_os_str().as_encoded_bytes())?;
        writeln!(self.out)?;

        Ok(())
    }

    pub fn var(&mut self, name: &str, value: &str) {
        writeln!(self.out, "{} = {}", name, value).unwrap();
    }

    pub fn rule(&mut self, name: &str, command: &str) {
        writeln!(self.out, "rule {}", name).unwrap();
        writeln!(self.out, "  command = {}", command).unwrap();
    }

    pub fn build(&mut self, rule: &str, input: &Path, output: &Path) {
        self.out.write_all(b"build ").unwrap();
        self.out
            .write_all(output.as_os_str().as_encoded_bytes())
            .unwrap();
        write!(self.out, ": {} ", rule).unwrap();
        self.out
            .write_all(input.as_os_str().as_encoded_bytes())
            .unwrap();
        self.out.write_all(b"\n").unwrap();
    }
}
