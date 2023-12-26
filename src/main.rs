use fake::{cli, Driver, DriverBuilder, RuleBuilder};

// TODO: Rethink the modularity of operators... maybe they should be traits instead of objects??
// Things they need to encapsulate include configuration options (which could, importantly, be
// shared between multiple operators) and setup/rule code (similarly shared?).

fn build_driver() -> Driver {
    let mut bld = DriverBuilder::default();

    let dahlia = bld.state("dahlia", &["fuse"]);
    let mrxl = bld.state("mrxl", &["mrxl"]);
    let calyx = bld.state("calyx", &["futil"]);
    let verilog = bld.state("verilog", &["sv", "v"]);

    let calyx_setup = bld.setup(
        RuleBuilder::default()
            .var("calyx_base", "/Users/asampson/cu/research/calyx")
            .var("calyx_exe", "$calyx_base/target/debug/calyx")
            .rule(
                "calyx-to-verilog",
                "$calyx_exe -l $calyx_base -b verilog $in -o $out",
            )
            .rule("calyx-to-calyx", "$calyx_exe -l $calyx_base $in -o $out")
            .build(),
    );

    bld.rule(Some(calyx_setup), calyx, verilog, "calyx-to-verilog");
    bld.rule(Some(calyx_setup), calyx, calyx, "calyx-to-calyx");

    let dahlia_setup = bld.setup(
        RuleBuilder::default()
            .var("dahlia_exec", "/Users/asampson/cu/research/dahlia/fuse")
            .rule(
                "dahlia-to-calyx",
                "$dahlia_exec -b calyx --lower -l error $in -o $out",
            )
            .build(),
    );

    bld.rule(Some(dahlia_setup), dahlia, calyx, "dahlia-to-calyx");

    let mrxl_setup = bld.setup(
        RuleBuilder::default()
            .var("mrxl_exec", "mrxl")
            .rule("mrxl-to-calyx", "$mrxl_exec $in > $out")
            .build(),
    );

    bld.rule(Some(mrxl_setup), mrxl, calyx, "mrxl-to-calyx");

    bld.build()
}

fn main() -> anyhow::Result<()> {
    let driver = build_driver();
    cli::cli(&driver)
}
