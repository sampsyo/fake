use fake::{Driver, DriverBuilder, Emitter};
use std::path::PathBuf;

fn calyx_build(emitter: &mut Emitter, input: PathBuf) -> PathBuf {
    writeln!(
        emitter.out,
        "run calyx -b verilog {}",
        input.to_string_lossy()
    )
    .unwrap();
    input
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
        |_| unimplemented!(),
        calyx_build,
    );
    bld.op(
        "compile Calyx internally",
        calyx,
        calyx,
        |_| unimplemented!(),
        |_, _| unimplemented!(),
    );
    bld.op(
        "compile Dahlia",
        dahlia,
        calyx,
        |_| unimplemented!(),
        |_, rsrc| {
            println!("run fuse");
            rsrc
        },
    );
    bld.op(
        "compile MrXL",
        mrxl,
        calyx,
        |_| unimplemented!(),
        |_, _| unimplemented!(),
    );

    bld.build()
}

fn main() {
    let driver = build_driver();
    driver.main();
}
