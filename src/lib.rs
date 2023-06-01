use cranelift_entity::{entity_impl, PrimaryMap};

/// The details about a given state.
pub struct StateData {
    name: String,
    extensions: Vec<String>,
}
/// A reference to a state.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct State(u32);
entity_impl!(State, "state");

type OpCall = fn(&dyn Resource) -> &dyn Resource;

/// An operation that transforms resources from one state to another.
pub struct OpData {
    name: String,
    input: State,
    output: State,
    call: OpCall,
}

/// A reference to an operation.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct Operation(u32);
entity_impl!(Operation, "operation");

pub trait Resource {
    fn as_str(&self) -> &str;
}

struct StringResource {
    value: String,
}

impl Resource for StringResource {
    fn as_str(&self) -> &str {
        &self.value
    }
}

#[derive(Default)]
pub struct Driver {
    pub states: PrimaryMap<State, StateData>,
    pub ops: PrimaryMap<Operation, OpData>,
}

impl Driver {
    pub fn add_state(&mut self, name: &str, extensions: &[&str]) -> State {
        self.states.push(StateData {
            name: name.to_string(),
            extensions: extensions.iter().map(|s| s.to_string()).collect(),
        })
    }

    pub fn add_op(
        &mut self,
        name: &str,
        input: State,
        output: State,
        call: OpCall,
    ) -> Operation {
        self.ops.push(OpData {
            name: name.to_string(),
            input,
            output,
            call,
        })
    }
}
