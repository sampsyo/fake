use crate::run::Emitter;
use camino::{Utf8Path, Utf8PathBuf};
use cranelift_entity::{entity_impl, PrimaryMap, SecondaryMap};
use pathdiff::diff_utf8_paths;

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
    pub setups: Vec<SetupRef>,
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
    fn build(&self, emitter: &mut Emitter, input: &str, output: &str) -> std::io::Result<()>;
}

type EmitBuildFn = fn(&mut Emitter, &str, &str) -> std::io::Result<()>;

impl EmitBuild for EmitBuildFn {
    fn build(&self, emitter: &mut Emitter, input: &str, output: &str) -> std::io::Result<()> {
        self(emitter, input, output)
    }
}

// TODO make this unnecessary...
/// A simple `build` emitter that just runs a Ninja rule.
pub struct EmitRuleBuild {
    pub rule_name: String,
}

impl EmitBuild for EmitRuleBuild {
    fn build(&self, emitter: &mut Emitter, input: &str, output: &str) -> std::io::Result<()> {
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

/// Get a version of `path` that works when the working directory is `base`. This is
/// opportunistically a relative path, but we can always fall back to an absolute path to make sure
/// the path still works.
pub fn relative_path(path: &Utf8Path, base: &Utf8Path) -> Utf8PathBuf {
    match diff_utf8_paths(path, base) {
        Some(p) => p,
        None => path
            .canonicalize_utf8()
            .expect("could not get absolute path"),
    }
}

#[derive(PartialEq)]
enum Destination {
    State(StateRef),
    Op(OpRef),
}

/// A Driver encapsulates a set of States and the Operations that can transform between them. It
/// contains all the machinery to perform builds in a given ecosystem.
pub struct Driver {
    pub setups: PrimaryMap<SetupRef, Setup>,
    pub states: PrimaryMap<StateRef, State>,
    pub ops: PrimaryMap<OpRef, Operation>,
    pub(crate) stdin_op: OpRef,
    pub(crate) stdout_op: OpRef,
}

impl Driver {
    /// Find a chain of Operations from the `start` state to the `end`, which may be a state or the
    /// final operation in the chain.
    fn find_path_segment(&self, start: StateRef, end: Destination) -> Option<Vec<OpRef>> {
        // Our start state is the input.
        let mut visited = SecondaryMap::<StateRef, bool>::new();
        visited[start] = true;

        // Build the incoming edges for each vertex.
        let mut breadcrumbs = SecondaryMap::<StateRef, Option<OpRef>>::new();

        // Breadth-first search.
        let mut state_queue: Vec<StateRef> = vec![start];
        while !state_queue.is_empty() {
            let cur_state = state_queue.remove(0);

            // Finish when we reach the goal vertex.
            if end == Destination::State(cur_state) {
                break;
            }

            // Traverse any edge from the current state to an unvisited state.
            for (op_ref, op) in self.ops.iter() {
                if op.input == cur_state && !visited[op.output] {
                    state_queue.push(op.output);
                    visited[op.output] = true;
                    breadcrumbs[op.output] = Some(op_ref);
                }

                // Finish when we reach the goal edge.
                if end == Destination::Op(op_ref) {
                    break;
                }
            }
        }

        // Traverse the breadcrumbs backward to build up the path back from output to input.
        let mut op_path: Vec<OpRef> = vec![];
        let mut cur_state = match end {
            Destination::State(state) => state,
            Destination::Op(op) => {
                op_path.push(op);
                self.ops[op].input
            }
        };
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

    /// Find a chain of operations from the `start` state to the `end` state, passing through each
    /// `through` operation in order.
    pub fn find_path(
        &self,
        start: StateRef,
        end: StateRef,
        through: &[OpRef],
    ) -> Option<Vec<OpRef>> {
        let mut cur_state = start;
        let mut op_path: Vec<OpRef> = vec![];

        // Build path segments through each through required operation.
        for op in through {
            let segment = self.find_path_segment(cur_state, Destination::Op(*op))?;
            op_path.extend(segment);
            cur_state = self.ops[*op].output;
        }

        // Build the final path segment to the destination state.
        let segment = self.find_path_segment(cur_state, Destination::State(end))?;
        op_path.extend(segment);

        Some(op_path)
    }

    /// Generate a filename with an extension appropriate for the given State.
    fn gen_name(&self, stem: &str, state: StateRef) -> Utf8PathBuf {
        // TODO avoid collisions in case we reuse extensions...
        let ext = &self.states[state].extensions[0];
        Utf8PathBuf::from(stem).with_extension(ext)
    }

    pub fn plan(&self, req: Request) -> Option<Plan> {
        // Find a path through the states.
        let path = self.find_path(req.start_state, req.end_state, &req.through)?;

        let mut steps: Vec<(OpRef, Utf8PathBuf)> = vec![];

        // Get the initial input filename and the stem to use to generate all intermediate filenames.
        let start_file = match req.start_file {
            Some(path) => relative_path(&path, &req.workdir),
            None => {
                // Use the special "stdin" operator to capture the input file.
                let filename = self.gen_name("stdin", self.ops[path[0]].input);
                steps.push((self.stdin_op, filename.clone()));
                filename
            }
        };
        let stem = start_file.file_stem().unwrap();

        // Generate filenames for each step.
        steps.extend(path.into_iter().map(|op| {
            let filename = self.gen_name(stem, self.ops[op].output);
            (op, filename)
        }));

        if let Some(end_file) = req.end_file {
            // If we have a specified output filename, use that instead of the generated one.
            // TODO Can we just avoid generating the unused filename in the first place?
            let last_step = steps.last_mut().expect("no steps");
            last_step.1 = relative_path(&end_file, &req.workdir);
            false
        } else {
            // Use the special "stdout" operator to show the output.
            steps.push((self.stdout_op, Utf8PathBuf::from("_stdout")));
            true
        };

        Some(Plan {
            start: start_file,
            steps,
            workdir: req.workdir,
        })
    }

    pub fn guess_state(&self, path: &Utf8Path) -> Option<StateRef> {
        let ext = path.extension()?;
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

    pub fn get_op(&self, name: &str) -> Option<OpRef> {
        self.ops
            .iter()
            .find(|(_, op_data)| op_data.name == name)
            .map(|(op, _)| op)
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
        setups: &[SetupRef],
        input: StateRef,
        output: StateRef,
        emit: T,
    ) -> OpRef {
        self.ops.push(Operation {
            name: name.into(),
            setups: setups.into(),
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
        setups: &[SetupRef],
        input: StateRef,
        output: StateRef,
        build: EmitBuildFn,
    ) -> OpRef {
        self.add_op(name, setups, input, output, build)
    }

    pub fn rule(
        &mut self,
        setups: &[SetupRef],
        input: StateRef,
        output: StateRef,
        rule_name: &str,
    ) -> OpRef {
        self.add_op(
            rule_name,
            setups,
            input,
            output,
            EmitRuleBuild {
                rule_name: rule_name.to_string(),
            },
        )
    }

    /// Add our built-in operations for capturing stdin and printing to stdout.
    fn builtin_ops(&mut self) -> (OpRef, OpRef) {
        let null_state = self.state("null", &[]);

        let stdin_setup = self.setup("stdin", |e| {
            e.rule("capture", "cat > $out")?;
            writeln!(e.out, "  pool = console")?;
            Ok(())
        });
        let stdin = self.op(
            "stdin",
            &[stdin_setup],
            null_state,
            null_state,
            |e, _, output| {
                writeln!(e.out, "build {}: capture", output)?;
                Ok(())
            },
        );

        let stdout_setup = self.setup("stdout", |e| {
            e.rule("show", "cat $in")?;
            writeln!(e.out, "  pool = console")?;
            Ok(())
        });
        let stdout = self.rule(&[stdout_setup], null_state, null_state, "show");

        (stdin, stdout)
    }

    pub fn build(mut self) -> Driver {
        let (stdin_op, stdout_op) = self.builtin_ops();
        Driver {
            setups: self.setups,
            states: self.states,
            ops: self.ops,
            stdin_op,
            stdout_op,
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
    pub start_file: Option<Utf8PathBuf>,

    /// The filename to write the output to, or None to print to stdout.
    pub end_file: Option<Utf8PathBuf>,

    /// A sequence of operators to route the conversion through.
    pub through: Vec<OpRef>,

    /// The working directory for the build.
    pub workdir: Utf8PathBuf,
}

#[derive(Debug)]
pub struct Plan {
    /// The input to the first step.
    pub start: Utf8PathBuf,

    /// The chain of operations to run and each step's output file.
    pub steps: Vec<(OpRef, Utf8PathBuf)>,

    /// The directory that the build will happen in.
    pub workdir: Utf8PathBuf,
}
