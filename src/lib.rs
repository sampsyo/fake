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

/// An operation that transforms resources from one state to another.
pub struct OpData {
    name: String,
    input: State,
    output: State,
    call: fn(&dyn Resource) -> dyn Resource,
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

pub struct Driver {
    states: PrimaryMap<State, StateData>,
    ops: PrimaryMap<Operation, OpData>,
}
