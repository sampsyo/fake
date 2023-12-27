use fake::{cli, Driver, DriverBuilder};

fn build_driver() -> Driver {
    let mut bld = DriverBuilder::default();

    let dahlia = bld.state("dahlia", &["fuse"]);
    let mrxl = bld.state("mrxl", &["mrxl"]);
    let calyx = bld.state("calyx", &["futil"]);
    let verilog = bld.state("verilog", &["sv", "v"]);

    // Calyx.
    let calyx_setup = bld.setup(|e, _r| {
        e.var("calyx_base", "/Users/asampson/cu/research/calyx");
        e.var("calyx_exe", "$calyx_base/target/debug/calyx");
        e.rule(
            "calyx-to-verilog",
            "$calyx_exe -l $calyx_base -b verilog $in -o $out",
        );
        e.rule("calyx-to-calyx", "$calyx_exe -l $calyx_base $in -o $out");
    });
    bld.rule(Some(calyx_setup), calyx, verilog, "calyx-to-verilog");
    bld.rule(Some(calyx_setup), calyx, calyx, "calyx-to-calyx");

    // Dahlia.
    let dahlia_setup = bld.setup(|e, _r| {
        e.var("dahlia_exec", "/Users/asampson/cu/research/dahlia/fuse");
        e.rule(
            "dahlia-to-calyx",
            "$dahlia_exec -b calyx --lower -l error $in -o $out",
        );
    });
    bld.rule(Some(dahlia_setup), dahlia, calyx, "dahlia-to-calyx");

    // MrXL.
    let mrxl_setup = bld.setup(|e, _r| {
        e.var("mrxl_exec", "mrxl");
        e.rule("mrxl-to-calyx", "$mrxl_exec $in > $out");
    });
    bld.rule(Some(mrxl_setup), mrxl, calyx, "mrxl-to-calyx");

    bld.build()
}

fn main() -> anyhow::Result<()> {
    let driver = build_driver();
    cli::cli(&driver)
}
