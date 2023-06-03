use fake::{Driver, DriverBuilder, Resource};

fn build_driver() -> Driver {
    let mut bld = DriverBuilder::default();

    let dahlia = bld.state("dahlia", &["fuse"]);
    let mrxl = bld.state("mrxl", &["mrxl"]);
    let calyx = bld.state("calyx", &["futil"]);
    let verilog = bld.state("verilog", &["sv"]);

    bld.op("compile Calyx to Verilog", calyx, verilog, |build, rsrc| {
        let path = build.file(rsrc);
        println!("run calyx -b verilog {}", path.to_string_lossy());
        Resource::File(path)
    });
    bld.op(
        "compile Calyx internally",
        calyx,
        calyx,
        |_, _| unimplemented!(),
    );
    bld.op("compile Dahlia", dahlia, calyx, |_, rsrc| {
        println!("run fuse");
        rsrc
    });
    bld.op("compile MrXL", mrxl, calyx, |_, _| unimplemented!());

    bld.build()
}

fn main() {
    let driver = build_driver();
    driver.main();
}
