use fake::{cli, Driver, DriverBuilder};

fn build_driver() -> Driver {
    let mut bld = DriverBuilder::new("fud2");

    // Calyx.
    let calyx = bld.state("calyx", &["futil"]);
    let verilog = bld.state("verilog", &["sv", "v"]);
    let calyx_setup = bld.setup("Calyx compiler", |e| {
        e.config_var("calyx_base", "calyx.base")?;
        e.config_var_or("calyx_exe", "calyx.exe", "$calyx_base/target/debug/calyx")?;
        e.rule(
            "calyx",
            "$calyx_exe -l $calyx_base -b $backend $args $in > $out",
        )?;
        Ok(())
    });
    bld.op(
        "calyx-to-verilog",
        &[calyx_setup],
        calyx,
        verilog,
        |e, input, output| {
            e.build_cmd(output, "calyx", &[input], &[])?;
            e.arg("backend", "verilog")?;
            Ok(())
        },
    );

    // Dahlia.
    let dahlia = bld.state("dahlia", &["fuse"]);
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
    let mrxl = bld.state("mrxl", &["mrxl"]);
    let mrxl_setup = bld.setup("MrXL compiler", |e| {
        e.var("mrxl_exec", "mrxl")?;
        e.rule("mrxl-to-calyx", "$mrxl_exec $in > $out")?;
        Ok(())
    });
    bld.rule(&[mrxl_setup], mrxl, calyx, "mrxl-to-calyx");

    // Shared machinery for RTL simulators.
    let dat = bld.state("dat", &["json"]);
    let sim_setup = bld.setup("RTL simulation", |e| {
        // Data conversion to and from JSON.
        e.config_var_or("python", "python", "python3")?;
        e.var(
            "json_dat",
            &format!("$python {}/json-dat.py", e.config_val("data")?),
        )?;
        e.rule("hex-data", "$json_dat --from-json $in $out")?;
        e.rule("json-data", "$json_dat --to-json $out $in")?;

        // The Verilog testbench.
        e.var("testbench", &format!("{}/tb.sv", e.config_val("data")?))?;

        // The input data file. `sim.data` is required.
        let data_name = e.config_val("sim.data")?;
        let data_path = e.external_path(data_name.as_ref());
        e.var("sim_data", data_path.as_str())?;

        // Produce the data directory.
        e.var("datadir", "sim_data")?;
        e.build("hex-data", "$sim_data", "$datadir")?;

        // More shared configuration.
        e.config_var_or("cycle_limit", "sim.cycle_limit", "500000000")?;

        Ok(())
    });

    // Icarus Verilog.
    let icarus_setup = bld.setup("Icarus Verilog", |e| {
        e.var("iverilog", "iverilog")?;
        e.rule("icarus-compile", "$iverilog -g2012 -o $out $testbench $in")?;
        e.rule(
            "icarus-sim",
            "./$bin +DATA=$datadir +CYCLE_LIMIT=$cycle_limit +NOTRACE=1 > $out",
        )?;
        Ok(())
    });
    bld.op(
        "icarus",
        &[calyx_setup, sim_setup, icarus_setup],
        calyx,
        dat,
        |e, input, output| {
            // Compile the Calyx to Verilog. We need to do this here (rather than making the op go
            // from Verilog) because Icarus requires the `--disable-verify` flag.
            let verilog_name = "sim.sv";
            e.build_cmd(verilog_name, "calyx", &[input], &[])?;
            e.arg("backend", "verilog")?;
            e.arg("args", "--disable-verify")?;

            // Compile the Verilog.
            let bin_name = "icarus_bin";
            e.build("icarus-compile", verilog_name, bin_name)?;

            // Run the simulation.
            e.build_cmd("sim.log", "icarus-sim", &[bin_name, "$datadir"], &[])?;
            e.arg("bin", bin_name)?;
            e.build_cmd(output, "json-data", &["$datadir", "sim.log"], &[])?;

            Ok(())
        },
    );

    // Verilator.
    let verilator_setup = bld.setup("Verilator", |e| {
        e.config_var_or("verilator", "verilator.exe", "verilator")?;
        e.config_var_or("cycle_limit", "sim.cycle_limit", "500000000")?;
        e.rule(
            "verilator-compile",
            "$verilator $in $testbench --trace --binary --top-module TOP -fno-inline -Mdir $out",
        )?;
        e.rule(
            "verilator-sim",
            "./$bin +DATA=$datadir +CYCLE_LIMIT=$cycle_limit +NOTRACE=1 > $out",
        )?;
        Ok(())
    });
    bld.op(
        "verilator",
        &[sim_setup, verilator_setup],
        verilog,
        dat,
        |e, input, output| {
            let out_dir = "verilator-out";
            e.build("verilator-compile", input, out_dir)?;

            e.build_cmd("sim.log", "verilator-sim", &[out_dir, "$datadir"], &[])?;
            e.arg("bin", &format!("{}/VTOP", out_dir))?;
            e.build_cmd(output, "json-data", &["$datadir", "sim.log"], &[])?;

            Ok(())
        },
    );

    // Interpreter.
    let debug = bld.state("debug", &[]); // A pseudo-state.
    let cider_setup = bld.setup("Cider interpreter", |e| {
        e.config_var_or("cider", "cider.exe", "$calyx_base/target/debug/cider")?;
        e.rule(
            "cider",
            "$cider -l $calyx_base --raw --data data.json $in > $out",
        )?;
        e.rule(
            "cider-debug",
            "$cider -l $calyx_base --data data.json $in debug || true",
        )?;
        e.arg("pool", "console")?;

        // TODO Can we reduce the duplication around `rsrc_dir` and `$python`?
        let rsrc_dir = e.config_val("data")?;
        e.var("interp-dat", &format!("{}/interp-dat.py", rsrc_dir))?;
        e.config_var_or("python", "python", "python3")?;
        e.rule("dat-to-interp", "$python $interp-dat --to-interp $in")?;
        e.rule(
            "interp-to-dat",
            "$python $interp-dat --from-interp $in $sim_data > $out",
        )?;
        e.build_cmd("data.json", "dat-to-interp", &["$sim_data"], &[])?;
        Ok(())
    });
    bld.op(
        "interp",
        &[sim_setup, calyx_setup, cider_setup],
        calyx,
        dat,
        |e, input, output| {
            let out_file = "interp_out.json";
            e.build_cmd(out_file, "cider", &[input], &["data.json"])?;
            e.build_cmd(output, "interp-to-dat", &[out_file], &["$sim_data"])?;
            Ok(())
        },
    );
    bld.op(
        "debug",
        &[sim_setup, calyx_setup, cider_setup],
        calyx,
        debug,
        |e, input, output| {
            e.build_cmd(output, "cider-debug", &[input], &["data.json"])?;
            Ok(())
        },
    );

    // Xilinx compilation.
    let xo = bld.state("xo", &["xo"]);
    let xclbin = bld.state("xclbin", &["xclbin"]);
    let xilinx_setup = bld.setup("Xilinx tools", |e| {
        // Locations for Vivado and Vitis installations.
        e.config_var("vivado_dir", "xilinx.vivado")?;
        e.config_var("vitis_dir", "xilinx.vitis")?;

        // Package a Verilog program as an `.xo` file.
        let rsrc_dir = e.config_val("data")?;
        e.var("gen_xo_tcl", &format!("{}/gen_xo.tcl", rsrc_dir))?;
        e.var("get_ports", &format!("{}/get-ports.py", rsrc_dir))?;
        e.config_var_or("python", "python", "python3")?;
        e.rule("gen-xo", "$vivado_dir/bin/vivado -mode batch -source $gen_xo_tcl -tclargs $out `$python $get_ports kernel.xml`")?;
        e.arg("pool", "console")?;  // Lets Ninja stream the tool output "live."

        // Compile an `.xo` file to an `.xclbin` file, which is where the actual EDA work occurs.
        e.config_var_or("xilinx_mode", "xilinx.mode", "hw_emu")?;
        e.config_var_or("platform", "xilinx.device", "xilinx_u50_gen3x16_xdma_201920_3")?;
        e.rule("compile-xclbin", "$vitis_dir/bin/v++ -g -t $xilinx_mode --platform $platform --save-temps --profile.data all:all:all --profile.exec all:all:all -lo $out $in")?;
        e.arg("pool", "console")?;

        Ok(())
    });
    bld.op(
        "xo",
        &[calyx_setup, xilinx_setup],
        calyx,
        xo,
        |e, input, output| {
            // Emit the Verilog itself in "synthesis mode."
            e.build_cmd("main.sv", "calyx", &[input], &[])?;
            e.arg("backend", "verilog")?;
            e.arg("args", "--synthesis -p external")?;

            // Extra ingredients for the `.xo` package.
            e.build_cmd("toplevel.v", "calyx", &[input], &[])?;
            e.arg("backend", "xilinx")?;
            e.build_cmd("kernel.xml", "calyx", &[input], &[])?;
            e.arg("backend", "xilinx-xml")?;

            // Package the `.xo`.
            e.build_cmd(
                output,
                "gen-xo",
                &[],
                &["main.sv", "toplevel.v", "kernel.xml"],
            )?;
            Ok(())
        },
    );
    bld.op("xclbin", &[xilinx_setup], xo, xclbin, |e, input, output| {
        e.build_cmd(output, "compile-xclbin", &[input], &[])?;
        Ok(())
    });

    // Xilinx execution.
    // TODO Only does `hw_emu` for now...
    let xrt_setup = bld.setup("Xilinx execution via XRT", |e| {
        // Generate `emconfig.json`.
        e.rule("emconfig", "$vitis_dir/bin/emconfigutil --platform $platform")?;
        e.build_cmd("emconfig.json", "emconfig", &[], &[])?;

        // A path to our stock `xrt.ini`.
        // TODO: This is where would set up for VCD generation (by generating a new `xrt.ini`).
        let rsrc_dir = e.config_val("data")?;
        e.var("xrt_ini", &format!("{}/xrt.ini", rsrc_dir))?;

        // Execute via the `xclrun` tool.
        e.config_var("xrt_dir", "xilinx.xrt")?;
        e.rule("xclrun", "bash -c 'source $vitis_dir/settings64.sh ; source $xrt_dir/setup.sh ; XRT_INI_PATH=$xrt_ini EMCONFIG_PATH=. XCL_EMULATION_MODE=$xilinx_mode $python -m fud.xclrun --out $out $in'")?;
        e.arg("pool", "console")?;

        Ok(())
    });
    bld.op(
        "xrt",
        &[xilinx_setup, sim_setup, xrt_setup],
        xclbin,
        dat,
        |e, input, output| {
            e.build_cmd(output, "xclrun", &[input, "$sim_data"], &["emconfig.json"])?;
            Ok(())
        },
    );

    bld.build()
}

fn main() -> anyhow::Result<()> {
    let driver = build_driver();
    cli::cli(&driver)
}
