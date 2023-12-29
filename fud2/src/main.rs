use fake::{cli, Driver, DriverBuilder};
use lazy_static_include::*;

lazy_static_include_bytes! {
    JSON_DAT => "data/json-dat.py",
    TB_SV => "data/tb.sv",
}

fn build_driver() -> Driver {
    let mut bld = DriverBuilder::default();

    let dahlia = bld.state("dahlia", &["fuse"]);
    let mrxl = bld.state("mrxl", &["mrxl"]);
    let calyx = bld.state("calyx", &["futil"]);
    let verilog = bld.state("verilog", &["sv", "v"]);
    let dat = bld.state("dat", &["json"]);

    // Calyx.
    // TODO: Currently hard-coding `--disable-verify`; this is only necessary for Icraus.
    let calyx_setup = bld.setup("Calyx compiler", |e| {
        e.config_var("calyx_base", "calyx.base")?;
        e.config_var_or("calyx_exe", "calyx.exe", "$calyx_base/target/debug/calyx")?;
        e.rule(
            "calyx-to-verilog",
            "$calyx_exe -l $calyx_base -b verilog --disable-verify $in > $out",
        )?;
        e.rule("calyx-to-calyx", "$calyx_exe -l $calyx_base $in -o $out")?;
        Ok(())
    });
    bld.rule(&[calyx_setup], calyx, verilog, "calyx-to-verilog");
    bld.rule(&[calyx_setup], calyx, calyx, "calyx-to-calyx");

    // Dahlia.
    let dahlia_setup = bld.setup("Dahlia compiler", |e| {
        e.var("dahlia_exec", "/Users/asampson/cu/research/dahlia/fuse")?;
        e.rule(
            "dahlia-to-calyx",
            "$dahlia_exec -b calyx --lower -l error $in -o $out",
        )?;
        Ok(())
    });
    bld.rule(&[dahlia_setup], dahlia, calyx, "dahlia-to-calyx");

    // MrXL.
    let mrxl_setup = bld.setup("MrXL compiler", |e| {
        e.var("mrxl_exec", "mrxl")?;
        e.rule("mrxl-to-calyx", "$mrxl_exec $in > $out")?;
        Ok(())
    });
    bld.rule(&[mrxl_setup], mrxl, calyx, "mrxl-to-calyx");

    // Icarus Verilog.
    let data_setup = bld.setup("data conversion for RTL simulation", |e| {
        e.add_file("json-dat.py", &JSON_DAT)?;
        e.rule("hex-data", "python3 json-dat.py --from-json $in $out")?;
        e.rule("json-data", "python3 json-dat.py --to-json $in $out")?;
        Ok(())
    });
    let icarus_setup = bld.setup("Icarus Verilog", |e| {
        e.add_file("tb.sv", &TB_SV)?;
        e.var("testbench", "tb.sv")?;

        // The input data file. `sim.data` is required.
        let data_name = e.config_val("sim.data");
        let data_path = e.external_path(data_name.as_ref());
        e.var("data", data_path.as_str())?;

        e.var("icarus_exec", "iverilog")?;
        e.var("datadir", "data")?;
        e.config_var_or("cycle_limit", "sim.cycle_limit", "500000000")?;
        e.rule(
            "icarus-compile",
            "$icarus_exec -g2012 -o $out $testbench $in",
        )?;
        e.rule(
            "icarus-sim",
            "./$bin +DATA=$datadir +CYCLE_LIMIT=$cycle_limit +NOTRACE=1",
        )?;

        Ok(())
    });
    bld.op(
        "icarus",
        &[data_setup, icarus_setup],
        verilog,
        dat,
        |e, input, output| {
            let bin_name = "icarus_bin";
            e.build("icarus-compile", input, bin_name)?;
            e.build("hex-data", "$data", "$datadir")?;

            // TODO Better utilities...
            writeln!(e.out, "build _sim: icarus-sim {} $datadir", bin_name)?;
            writeln!(e.out, "  bin = {}", bin_name)?;
            writeln!(e.out, "build {}: json-data $datadir | _sim", output)?;

            Ok(())
        },
    );

    bld.build()
}

fn main() -> anyhow::Result<()> {
    let driver = build_driver();
    cli::cli(&driver)
}
