use fake::{Driver, Operation, Resource, State};

fn main() {
    println!("Hello, world!");

    let mut driver = Driver::default();
    let calyx = driver.add_state("calyx", &["futil"]);
    let verilog = driver.add_state("verilog", &["sv"]);

    let op = driver.add_op("compile Calyx to Verilog", calyx, verilog, |_| {
        println!("compiling!");
        unimplemented!()
    });
}
