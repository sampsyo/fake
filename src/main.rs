use fake::DriverBuilder;

fn main() {
    println!("Hello, world!");

    let mut bld = DriverBuilder::default();

    let dahlia = bld.state("dahlia", &["fuse"]);
    let mrxl = bld.state("mrxl", &["mrxl"]);
    let calyx = bld.state("calyx", &["futil"]);
    let verilog = bld.state("verilog", &["sv"]);

    bld.op(
        "compile Calyx to Verilog",
        calyx,
        verilog,
        |_| unimplemented!(),
    );
    bld.op(
        "compile Calyx internally",
        calyx,
        calyx,
        |_| unimplemented!(),
    );
    bld.op("compile Dahlia", dahlia, calyx, |_| unimplemented!());
    bld.op("compile MrXL", mrxl, calyx, |_| unimplemented!());

    let driver = bld.build();
    let seq = driver.plan(dahlia, verilog).unwrap();
    for step in seq {
        println!("{}: {}", step, driver.ops[step].name);
    }
}
