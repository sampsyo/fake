use fake::{cli, Driver, DriverBuilder, Emitter};
use std::path::Path;

// TODO: Rethink the modularity of operators... maybe they should be traits instead of objects??
// Things they need to encapsulate include configuration options (which could, importantly, be
// shared between multiple operators) and setup/rule code (similarly shared?).

fn calyx_rules(emitter: &mut Emitter) {
    // TODO. something about configurable variables
    // TODO utilities for Ninja generation, or use a library?
    writeln!(
        emitter.out,
        "calyx_base = /Users/asampson/cu/research/calyx"
    )
    .unwrap();
    writeln!(emitter.out, "calyx_exe = $calyx_base/target/debug/calyx").unwrap();
    writeln!(emitter.out, "rule calyx").unwrap();
    writeln!(
        emitter.out,
        "  command = $calyx_exe -l $calyx_base -b verilog $in -o $out"
    )
    .unwrap();
}

fn calyx_build(emitter: &mut Emitter, input: &Path, output: &Path) {
    writeln!(
        emitter.out,
        "build {}: calyx {}",
        output.to_string_lossy(),
        input.to_string_lossy(),
    )
    .unwrap();
}

fn build_driver() -> Driver {
    let mut bld = DriverBuilder::default();

    let dahlia = bld.state("dahlia", &["fuse"]);
    let mrxl = bld.state("mrxl", &["mrxl"]);
    let calyx = bld.state("calyx", &["futil"]);
    let verilog = bld.state("verilog", &["sv"]);

    bld.op(
        "compile Calyx to Verilog",
        calyx,
        verilog,
        calyx_rules,
        calyx_build,
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
