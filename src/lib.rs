use cranelift_entity::{entity_impl, PrimaryMap, SecondaryMap};
use std::path::{Path, PathBuf};
use std::io::{Read, BufRead};

pub mod cli;

/// The details about a given state.
pub struct StateData {
    pub name: String,
    pub extensions: Vec<String>,
}
/// A reference to a state.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct State(u32);
entity_impl!(State, "state");

impl StateData {
    /// Check whether a filename extension indicates this state.
    fn ext_matches(&self, ext: &str) -> bool {
        self.extensions.iter().any(|e| e == ext)
    }
}

type OpCall = fn(Box<dyn Resource>) -> Box<dyn Resource>;

/// An operation that transforms resources from one state to another.
pub struct OpData {
    pub name: String,
    pub input: State,
    pub output: State,
    pub call: OpCall,
}

/// A reference to an operation.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct Operation(u32);
entity_impl!(Operation, "operation");

pub trait Resource {
    fn read(self) -> String;
    fn file(self) -> PathBuf;
    fn open(self) -> Box<dyn BufRead>;
}

struct StringResource {
    value: String,
}

struct FileResource {
    path: PathBuf,
}

impl Resource for StringResource {
    fn read(self) -> String {
        self.value
    }

    fn file(self) -> PathBuf {
        unimplemented!("needs a temporary file")
    }

    fn open(self) -> Box<dyn BufRead> {
        let cursor = std::io::Cursor::new(self.value);
        Box::new(cursor)
    }
}

impl Resource for FileResource {
    fn read(self) -> String {
        std::fs::read_to_string(self.path).unwrap()
    }

    fn file(self) -> PathBuf {
        self.path
    }

    fn open(self) -> Box<dyn BufRead> {
        let file = std::fs::File::open(self.path).unwrap();
        Box::new(std::io::BufReader::new(file))
    }
}

pub struct Driver {
    pub states: PrimaryMap<State, StateData>,
    pub ops: PrimaryMap<Operation, OpData>,
}

impl Driver {
    pub fn plan(&self, input: State, output: State) -> Option<Vec<Operation>> {
        // Our start state is the input.
        let mut visited = SecondaryMap::<State, bool>::new();
        visited[input] = true;

        // Build the incoming edges for each vertex.
        let mut breadcrumbs = SecondaryMap::<State, Option<Operation>>::new();

        // Breadth-first search.
        let mut state_queue: Vec<State> = vec![input];
        while !state_queue.is_empty() {
            let cur_state = state_queue.remove(0);

            // Finish when we reach the goal.
            if cur_state == output {
                break;
            }

            // Traverse any edge from the current state to an unvisited state.
            for (op, opdata) in self.ops.iter() {
                if opdata.input == cur_state && !visited[opdata.output] {
                    state_queue.push(opdata.output);
                    visited[opdata.output] = true;
                    breadcrumbs[opdata.output] = Some(op);
                }
            }
        }

        // Traverse the breadcrumbs backward to build up the path back from output to input.
        let mut op_path: Vec<Operation> = vec![];
        let mut cur_state = output;
        while cur_state != input {
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

    pub fn guess_state(&self, path: &Path) -> Option<State> {
        let ext = path.extension()?.to_str()?;
        self.states
            .iter()
            .find(|(_, state_data)| state_data.ext_matches(ext))
            .map(|(state, _)| state)
    }

    pub fn get_state(&self, name: &str) -> Option<State> {
        self.states
            .iter()
            .find(|(_, state_data)| state_data.name == name)
            .map(|(state, _)| state)
    }

    pub fn main(&self) {
        cli::cli(self);
    }
}

#[derive(Default)]
pub struct DriverBuilder {
    states: PrimaryMap<State, StateData>,
    ops: PrimaryMap<Operation, OpData>,
}

impl DriverBuilder {
    pub fn state(&mut self, name: &str, extensions: &[&str]) -> State {
        self.states.push(StateData {
            name: name.to_string(),
            extensions: extensions.iter().map(|s| s.to_string()).collect(),
        })
    }

    pub fn op(&mut self, name: &str, input: State, output: State, call: OpCall) -> Operation {
        self.ops.push(OpData {
            name: name.to_string(),
            input,
            output,
            call,
        })
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
    pub input: State,
    pub output: State,
}
