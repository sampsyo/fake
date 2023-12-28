use crate::run::Emitter;
use cranelift_entity::{entity_impl, PrimaryMap, SecondaryMap};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// A State is a type of file that Operations produce or consume.
pub struct State {
    pub name: String,
    pub extensions: Vec<String>,
}

/// A reference to a State.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct StateRef(u32);
entity_impl!(StateRef, "state");

/// An Operation transforms files from one State to another.
pub struct Operation {
    pub name: String,
    pub input: StateRef,
    pub output: StateRef,
    pub setup: Option<SetupRef>,
    pub emit: Box<dyn EmitBuild>,
}

/// A reference to an Operation.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct OpRef(u32);
entity_impl!(OpRef, "op");

/// A Setup runs at configuration time and produces Ninja machinery for Operations.
pub struct Setup {
    pub name: String,
    pub emit: Box<dyn EmitSetup>,
}

/// A reference to a Setup.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct SetupRef(u32);
entity_impl!(SetupRef, "setup");

impl State {
    /// Check whether a filename extension indicates this state.
    fn ext_matches(&self, ext: &str) -> bool {
        self.extensions.iter().any(|e| e == ext)
    }
}

/// Code to emit a Ninja `build` command.
pub trait EmitBuild {
    fn build(&self, emitter: &mut Emitter, input: &Path, output: &Path) -> std::io::Result<()>;
}

type EmitBuildFn = fn(&mut Emitter, &Path, &Path) -> std::io::Result<()>;

impl EmitBuild for EmitBuildFn {
    fn build(&self, emitter: &mut Emitter, input: &Path, output: &Path) -> std::io::Result<()> {
        self(emitter, input, output)
    }
}

/// A simple `build` emitter that just runs a Ninja rule.
pub struct EmitRuleBuild {
    pub rule_name: String,
}

impl EmitBuild for EmitRuleBuild {
    fn build(&self, emitter: &mut Emitter, input: &Path, output: &Path) -> std::io::Result<()> {
        emitter.build(&self.rule_name, input, output)
    }
}

/// Code to emit Ninja code at the setup stage.
pub trait EmitSetup {
    fn setup(&self, emitter: &mut Emitter) -> std::io::Result<()>;
}

type EmitSetupFn = fn(&mut Emitter) -> std::io::Result<()>;

impl EmitSetup for EmitSetupFn {
    fn setup(&self, emitter: &mut Emitter) -> std::io::Result<()> {
        self(emitter)
    }
}

/// A Driver encapsulates a set of States and the Operations that can transform between them. It
/// contains all the machinery to perform builds in a given ecosystem.
pub struct Driver {
    pub setups: PrimaryMap<SetupRef, Setup>,
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
                if op.input == cur_state && !visited[op.output] {
                    state_queue.push(op.output);
                    visited[op.output] = true;
                    breadcrumbs[op.output] = Some(op_ref);
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
                    cur_state = self.ops[op].input;
                }
                None => return None,
            }
        }
        op_path.reverse();

        Some(op_path)
    }

    /// Generate a filename with an extension appropriate for the given State.
    fn gen_name(&self, stem: &OsStr, state: StateRef) -> PathBuf {
        // TODO avoid collisions in case we reuse extensions...
        let ext = &self.states[state].extensions[0];
        PathBuf::from(stem).with_extension(ext)
    }

    pub fn plan(&self, req: Request) -> Option<Plan> {
        // Find a path through the states.
        let path = self.find_path(req.start_state, req.end_state)?;

        // Get the initial input filename and the stem to use to generate all intermediate filenames.
        let (stdin, start_file) = match req.start_file {
            Some(path) => (false, path),
            None => (
                true,
                self.gen_name(OsStr::new("stdin"), self.ops[path[0]].input),
            ),
        };
        let stem = start_file.file_stem().unwrap();

        // Generate filenames for each step.
        let mut steps: Vec<_> = path
            .into_iter()
            .map(|op| {
                let filename = self.gen_name(stem, self.ops[op].output);
                (op, filename)
            })
            .collect();

        // If we have a specified output filename, use that instead of the generated one.
        // TODO this is ugly
        let stdout = if let Some(end_file) = req.end_file {
            let last_step = steps.last_mut().expect("no steps");
            last_step.1 = end_file;
            false
        } else {
            true
        };

        Some(Plan {
            start: start_file,
            steps,
            stdin,
            stdout,
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
    setups: PrimaryMap<SetupRef, Setup>,
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

    fn add_op<T: EmitBuild + 'static>(
        &mut self,
        name: &str,
        setup: Option<SetupRef>,
        input: StateRef,
        output: StateRef,
        emit: T,
    ) -> OpRef {
        self.ops.push(Operation {
            name: name.to_string(),
            setup,
            input,
            output,
            emit: Box::new(emit),
        })
    }

    pub fn add_setup<T: EmitSetup + 'static>(&mut self, name: &str, emit: T) -> SetupRef {
        self.setups.push(Setup {
            name: name.into(),
            emit: Box::new(emit),
        })
    }

    pub fn setup(&mut self, name: &str, func: EmitSetupFn) -> SetupRef {
        self.add_setup(name, func)
    }

    pub fn op(
        &mut self,
        name: &str,
        setup: Option<SetupRef>,
        input: StateRef,
        output: StateRef,
        build: EmitBuildFn,
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
            EmitRuleBuild {
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

/// A request to the Driver directing it what to build.
#[derive(Debug)]
pub struct Request {
    /// The input format.
    pub start_state: StateRef,

    /// The output format to produce.
    pub end_state: StateRef,

    /// The filename to read the input from, or None to read from stdin.
    pub start_file: Option<PathBuf>,

    /// The filename to write the output to, or None to print to stdout.
    pub end_file: Option<PathBuf>,
}

#[derive(Debug)]
pub struct Plan {
    /// The input to the first step.
    pub start: PathBuf,

    /// The chain of operations to run and each step's output file.
    pub steps: Vec<(OpRef, PathBuf)>,

    /// Capture the first input from stdin.
    pub stdin: bool,

    /// Emit the last file to stdout.
    pub stdout: bool,
}
