use fake::{Driver, Operation, Resource, State};

fn main() {
    println!("Hello, world!");

    let mut driver = Driver::default();

    let dahlia = driver.add_state("dahlia", &["fuse"]);
    let mrxl = driver.add_state("mrxl", &["mrxl"]);
    let calyx = driver.add_state("calyx", &["futil"]);
    let verilog = driver.add_state("verilog", &["sv"]);

    driver.add_op(
        "compile Calyx to Verilog",
        calyx,
        verilog,
        |_| unimplemented!(),
    );
    driver.add_op(
        "compile Calyx internally",
        calyx,
        calyx,
        |_| unimplemented!(),
    );
    driver.add_op("compile Dahlia", dahlia, calyx, |_| unimplemented!());
    driver.add_op("compile MrXL", mrxl, calyx, |_| unimplemented!());
}
