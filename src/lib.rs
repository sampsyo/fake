use cranelift_entity::{entity_impl, PrimaryMap, SecondaryMap};
use std::collections::HashSet;
use std::ffi::OsStr;
use std::io::Write;
use std::path::{Path, PathBuf};

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

type EmitRules = fn(&mut Emitter) -> ();
type EmitBuild = fn(&mut Emitter, &Path, &Path) -> ();

/// An operation that transforms files from one state to another.
pub trait Operation {
    fn name(&self) -> &str;
    fn input(&self) -> StateRef;
    fn output(&self) -> StateRef;
    fn rules(&self, emitter: &mut Emitter) -> ();
    fn build(&self, emitter: &mut Emitter, input: &Path, output: &Path) -> ();
}

/// An Operation that just encapulates data.
/// TODO do we need this?
pub struct SimpleOp {
    pub name: String,
    pub input: StateRef,
    pub output: StateRef,
    pub rules: EmitRules,
    pub build: EmitBuild,
}

impl Operation for SimpleOp {
    fn name(&self) -> &str {
        &self.name
    }

    fn input(&self) -> StateRef {
        self.input
    }

    fn output(&self) -> StateRef {
        self.output
    }

    fn rules(&self, emitter: &mut Emitter) -> () {
        (self.rules)(emitter)
    }

    fn build(&self, emitter: &mut Emitter, input: &Path, output: &Path) -> () {
        (self.build)(emitter, input, output)
    }
}

/// A reference to an operation.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct OpRef(u32);
entity_impl!(OpRef, "operation");

pub struct Driver {
    pub states: PrimaryMap<StateRef, State>,
    pub ops: PrimaryMap<OpRef, Box<dyn Operation>>,
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
            for (op, opdata) in self.ops.iter() {
                if opdata.input() == cur_state && !visited[opdata.output()] {
                    state_queue.push(opdata.output());
                    visited[opdata.output()] = true;
                    breadcrumbs[opdata.output()] = Some(op);
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
                    cur_state = self.ops[op].input();
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
        let ext = &self.states[op.output()].extensions[0];

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
    states: PrimaryMap<StateRef, State>,
    ops: PrimaryMap<OpRef, Box<dyn Operation>>,
}

impl DriverBuilder {
    pub fn state(&mut self, name: &str, extensions: &[&str]) -> StateRef {
        self.states.push(State {
            name: name.to_string(),
            extensions: extensions.iter().map(|s| s.to_string()).collect(),
        })
    }

    pub fn op(
        &mut self,
        name: &str,
        input: StateRef,
        output: StateRef,
        rules: EmitRules,
        build: EmitBuild,
    ) -> OpRef {
        self.ops.push(Box::new(SimpleOp {
            name: name.to_string(),
            input,
            output,
            rules,
            build,
        }))
    }

    pub fn build(self) -> Driver {
        Driver {
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

pub struct Emitter {
    pub out: Box<dyn Write>,
}

impl Emitter {
    pub fn new(out: Box<dyn Write>) -> Self {
        Self { out }
    }

    pub fn emit(&mut self, driver: &Driver, plan: Plan) -> Result<(), std::io::Error> {
        // Emit the rules for each operation used in the plan, only once.
        let mut seen_ops = HashSet::<OpRef>::new();
        for (op, _) in &plan.steps {
            if seen_ops.insert(*op) {
                writeln!(self.out, "# {}", driver.ops[*op].name()).unwrap();
                let op = &driver.ops[*op];
                op.rules(self);
                writeln!(self.out)?;
            }
        }

        // Emit the build commands for each step in the plan.
        writeln!(self.out, "# build targets")?;
        let mut last_file = plan.start;
        for (op, out_file) in plan.steps {
            let op = &driver.ops[op];
            op.build(self, &last_file, &out_file);
            last_file = out_file;
        }

        // Mark the last file as the default target.
        writeln!(self.out)?;
        writeln!(self.out, "default {}", last_file.display())?; // TODO pass through bytes, not `display`

        Ok(())
    }

    /// Print the `build.ninja` file to stdout.
    pub fn emit_to_stdout(driver: &Driver, plan: Plan) -> Result<(), std::io::Error> {
        let mut emitter = Self::new(Box::new(std::io::stdout()));
        emitter.emit(driver, plan)
    }

    /// Ensure that a directory exists and write `build.ninja` inside it.
    pub fn emit_to_dir(driver: &Driver, plan: Plan, dir: &Path) -> Result<(), std::io::Error> {
        std::fs::create_dir_all(dir)?;
        let ninja_path = dir.join("build.ninja");
        let ninja_file = std::fs::File::create(ninja_path)?;

        let mut emitter = Self::new(Box::new(ninja_file));
        emitter.emit(driver, plan)
    }
}
