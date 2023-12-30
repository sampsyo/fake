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

    // Shared machinery for RTL simulators.
    let sim_setup = bld.setup("RTL simulation", |e| {
        // Data conversion to and from JSON.
        e.add_file("json-dat.py", &JSON_DAT)?;
        e.rule("hex-data", "python3 json-dat.py --from-json $in $out")?;
        e.rule("json-data", "python3 json-dat.py --to-json $in $out")?;

        // The Verilog testbench.
        // TODO I wish we could somehow refer to pre-existing files… would make "emit" mode nicer.
        e.add_file("tb.sv", &TB_SV)?;
        e.var("testbench", "tb.sv")?;

        // The input data file. `sim.data` is required.
        let data_name = e.config_val("sim.data");
        let data_path = e.external_path(data_name.as_ref());
        e.var("sim_data", data_path.as_str())?;

        // More shared configuration.
        e.config_var_or("cycle_limit", "sim.cycle_limit", "500000000")?;

        Ok(())
    });

    // Icarus Verilog.
    let icarus_setup = bld.setup("Icarus Verilog", |e| {
        e.var("iverilog", "iverilog")?;
        e.var("datadir", "data")?;
        e.rule("icarus-compile", "$iverilog -g2012 -o $out $testbench $in")?;
        e.rule(
            "icarus-sim",
            "./$bin +DATA=$datadir +CYCLE_LIMIT=$cycle_limit +NOTRACE=1",
        )?;
        Ok(())
    });
    bld.op(
        "icarus",
        &[sim_setup, icarus_setup],
        verilog,
        dat,
        |e, input, output| {
            let bin_name = "icarus_bin";
            e.build("icarus-compile", input, bin_name)?;
            e.build("hex-data", "$sim_data", "$datadir")?;

            e.build_cmd("_sim", "icarus-sim", &[bin_name, "$datadir"], &[])?;
            e.arg("bin", bin_name)?;
            e.build_cmd(output, "json-data", &["$datadir"], &["_sim"])?;

            Ok(())
        },
    );

    // Verilator.
    let verilator_setup = bld.setup("Verilator", |e| {
        e.var("verilator", "verilator")?;
        e.var("datadir", "data")?;
        e.config_var_or("cycle_limit", "sim.cycle_limit", "500000000")?;
        e.rule(
            "verilator-compile",
            "$verilator $in $testbench --trace --binary --top-module TOP -fno-inline -Mdir $out",
        )?;
        e.rule(
            "verilator-sim",
            "./$bin +DATA=$datadir +CYCLE_LIMIT=$cycle_limit +NOTRACE=1",
        )?;
        Ok(())
    });
    bld.op(
        "verilator",
        &[sim_setup, verilator_setup],
        verilog,
        dat,
        |e, input, output| {
            // TODO share as much as possible with Icarus...
            let out_dir = "verilator-out";
            e.build("verilator-compile", input, out_dir)?;
            e.build("hex-data", "$sim_data", "$datadir")?;

            e.build_cmd("_sim", "verilator-sim", &[out_dir, "$datadir"], &[])?;
            e.arg("bin", &format!("{}/VTOP", out_dir))?;
            e.build_cmd(output, "json-data", &["$datadir"], &["_sim"])?;

            Ok(())
        },
    );

    bld.build()
}

fn main() -> anyhow::Result<()> {
    let driver = build_driver();
    cli::cli(&driver)
}
