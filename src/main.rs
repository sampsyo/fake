use fake::{Driver, DriverBuilder};

fn build_driver() -> Driver {
    let mut bld = DriverBuilder::default();

    let dahlia = bld.state("dahlia", &["fuse"]);
    let mrxl = bld.state("mrxl", &["mrxl"]);
    let calyx = bld.state("calyx", &["futil"]);
    let verilog = bld.state("verilog", &["sv"]);

    bld.op("compile Calyx to Verilog", calyx, verilog, |rsrc| {
        println!("run calyx -b verilog");
        rsrc
    });
    bld.op(
        "compile Calyx internally",
        calyx,
        calyx,
        |_| unimplemented!(),
    );
    bld.op("compile Dahlia", dahlia, calyx, |rsrc| {
        println!("run fuse");
        rsrc
    });
    bld.op("compile MrXL", mrxl, calyx, |_| unimplemented!());

    bld.build()
}

fn main() {
    let driver = build_driver();
    driver.main();
}
