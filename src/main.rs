use fake::{cli, Driver, DriverBuilder};

// TODO: Rethink the modularity of operators... maybe they should be traits instead of objects??
// Things they need to encapsulate include configuration options (which could, importantly, be
// shared between multiple operators) and setup/rule code (similarly shared?).

fn build_driver() -> Driver {
    let mut bld = DriverBuilder::default();

    let dahlia = bld.state("dahlia", &["fuse"]);
    let mrxl = bld.state("mrxl", &["mrxl"]);
    let calyx = bld.state("calyx", &["futil"]);
    let verilog = bld.state("verilog", &["sv", "v"]);

    let calyx_setup = bld.setup_stanza(
        "calyx_base = /Users/asampson/cu/research/calyx
calyx_exe = $calyx_base/target/debug/calyx
rule calyx-to-verilog
  command = $calyx_exe -l $calyx_base -b verilog $in -o $out
rule calyx-to-calyx
  command = $calyx_exe -l $calyx_base $in -o $out",
    );
    // TODO: the two backends could also be selected with a Ninja variable...

    bld.rule(
        "compile Calyx to Verilog",
        Some(calyx_setup),
        calyx,
        verilog,
        "calyx-to-verilog",
    );
    bld.rule(
        "compile Calyx internally",
        Some(calyx_setup),
        calyx,
        calyx,
        "calyx-to-calyx",
    );

    let dahlia_setup = bld.setup_stanza(
        "dahlia_exec = /Users/asampson/cu/research/dahlia/fuse
rule dahlia-to-calyx
  command = $dahlia_exec -b calyx --lower -l error $in -o $out",
    );

    bld.rule(
        "compile Dahlia to Calyx",
        Some(dahlia_setup),
        dahlia,
        calyx,
        "dahlia-to-calyx",
    );

    let mrxl_setup = bld.setup_stanza(
        "mrxl_exec = mrxl
rule mrxl-to-calyx
  command = $mrxl_exec $in > $out",
    );

    bld.rule(
        "compile MrXL to Calyx",
        Some(mrxl_setup),
        mrxl,
        calyx,
        "mrxl-to-calyx",
    );

    bld.build()
}

fn main() -> anyhow::Result<()> {
    let driver = build_driver();
    cli::cli(&driver)
}
