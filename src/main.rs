use fake::{cli, Driver, DriverBuilder};

// TODO: Rethink the modularity of operators... maybe they should be traits instead of objects??
// Things they need to encapsulate include configuration options (which could, importantly, be
// shared between multiple operators) and setup/rule code (similarly shared?).

fn build_driver() -> Driver {
    let mut bld = DriverBuilder::default();

    let dahlia = bld.state("dahlia", &["fuse"]);
    let mrxl = bld.state("mrxl", &["mrxl"]);
    let calyx = bld.state("calyx", &["futil"]);
    let verilog = bld.state("verilog", &["sv"]);

    bld.rule(
        "compile Calyx to Verilog",
        calyx,
        verilog,
        "calyx",
        "calyx_base = /Users/asampson/cu/research/calyx
calyx_exe = $calyx_base/target/debug/calyx
rule calyx
  command = $calyx_exe -l $calyx_base -b verilog $in -o $out",
    );
    bld.op(
        "compile Calyx internally",
        calyx,
        calyx,
        |_| unimplemented!(),
        |_, _, _| unimplemented!(),
    );
    bld.op(
        "compile Dahlia",
        dahlia,
        calyx,
        |_| unimplemented!(),
        |_, _, _| {
            println!("run fuse");
        },
    );
    bld.op(
        "compile MrXL",
        mrxl,
        calyx,
        |_| unimplemented!(),
        |_, _, _| unimplemented!(),
    );

    bld.build()
}

fn main() -> anyhow::Result<()> {
    let driver = build_driver();
    cli::cli(&driver)
}
