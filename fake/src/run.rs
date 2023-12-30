use crate::config;
use crate::driver::{relative_path, Driver, OpRef, Plan, SetupRef, StateRef};
use camino::{Utf8Path, Utf8PathBuf};
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::process::Command;

pub struct Run<'a> {
    pub driver: &'a Driver,
    pub plan: Plan,
    pub config_data: figment::Figment,
    pub global_config: config::GlobalConfig,
}

impl<'a> Run<'a> {
    pub fn new(driver: &'a Driver, plan: Plan) -> Self {
        let config_data = config::load_config();
        let global_config: config::GlobalConfig =
            config_data.extract().expect("failed to load config");
        Self {
            driver,
            plan,
            config_data,
            global_config,
        }
    }

    /// Just print the plan for debugging purposes.
    pub fn show(self) {
        println!("start: {}", self.plan.start);
        for (op, file) in self.plan.steps {
            if op == self.driver.stdin_op {
                println!("{}: (stdin) -> {}", op, file);
            } else if op == self.driver.stdout_op {
                println!("{}: (stdout)", op);
            } else {
                println!("{}: {} -> {}", op, self.driver.ops[op].name, file);
            }
        }
    }

    /// Print a GraphViz representation of the plan.
    pub fn show_dot(self) {
        println!("digraph plan {{");
        println!("  node[shape=box];");

        // Record the states and ops that are actually used in the plan.
        let mut states: HashMap<StateRef, String> = HashMap::new();
        let mut ops: HashSet<OpRef> = HashSet::new();
        let first_op = self.plan.steps[0].0;
        states.insert(self.driver.ops[first_op].input, self.plan.start.to_string());
        for (op, file) in &self.plan.steps {
            states.insert(self.driver.ops[*op].output, file.to_string());
            ops.insert(*op);
        }

        // Show all states.
        for (state_ref, state) in self.driver.states.iter() {
            // Hide our "special" state for stdin/stdout.
            if state_ref == self.driver.ops[self.driver.stdin_op].input {
                continue;
            }

            print!("  {} [", state_ref);
            if let Some(filename) = states.get(&state_ref) {
                print!(
                    "label=\"{}\n{}\" penwidth=3 fillcolor=gray style=filled",
                    state.name, filename
                );
            } else {
                print!("label=\"{}\"", state.name);
            }
            println!("];");
        }

        // Show all operations.
        for (op_ref, op) in self.driver.ops.iter() {
            // Don't bother showing our "special" operations.
            if op_ref == self.driver.stdin_op || op_ref == self.driver.stdout_op {
                continue;
            }

            print!("  {} -> {} [label=\"{}\"", op.input, op.output, op.name);
            if ops.contains(&op_ref) {
                print!(" penwidth=3");
            }
            println!("];");
        }

        println!("}}");
    }

    /// Print the `build.ninja` file to stdout.
    pub fn emit_to_stdout(self) -> Result<(), std::io::Error> {
        self.emit(std::io::stdout())
    }

    /// Ensure that a directory exists and write `build.ninja` inside it.
    pub fn emit_to_dir(self, dir: &Utf8Path) -> Result<(), std::io::Error> {
        std::fs::create_dir_all(dir)?;
        let ninja_path = dir.join("build.ninja");
        let ninja_file = std::fs::File::create(ninja_path)?;

        self.emit(ninja_file)
    }

    /// Emit `build.ninja` to a temporary directory and then actually execute ninja.
    pub fn emit_and_run(self, dir: &Utf8Path) -> Result<(), std::io::Error> {
        // TODO: This workaround for lifetime stuff in the config isn't great.
        let keep = self.global_config.keep_build_dir;
        let ninja = self.global_config.ninja.clone();
        let stdout = self.plan.steps.last().unwrap().0 == self.driver.stdout_op;

        let stale_dir = dir.exists();
        self.emit_to_dir(dir)?;

        // Run `ninja` in the working directory.
        let mut cmd = Command::new(ninja);
        cmd.current_dir(dir);
        if stdout {
            // When we're printing to stdout, suppress Ninja's output.
            cmd.arg("--quiet");
        }
        cmd.status()?;

        // Remove the temporary directory unless it already existed at the start *or* the user specified `--keep`.
        if !keep && !stale_dir {
            std::fs::remove_dir_all(dir)?;
        }

        Ok(())
    }

    fn emit<T: Write + 'static>(self, out: T) -> Result<(), std::io::Error> {
        let mut emitter =
            Emitter::new(out, self.config_data, self.global_config, self.plan.workdir);

        // Emit the setup for each operation used in the plan, only once.
        let mut done_setups = HashSet::<SetupRef>::new();
        for (op, _) in &self.plan.steps {
            for setup in &self.driver.ops[*op].setups {
                if done_setups.insert(*setup) {
                    let setup = &self.driver.setups[*setup];
                    writeln!(emitter.out, "# {}", setup.name)?; // TODO more descriptive name
                    setup.emit.setup(&mut emitter)?;
                    writeln!(emitter.out)?;
                }
            }
        }

        // Emit the build commands for each step in the plan.
        emitter.comment("build targets")?;
        let mut last_file = self.plan.start;
        for (op, out_file) in self.plan.steps {
            let op = &self.driver.ops[op];
            op.emit
                .build(&mut emitter, last_file.as_str(), out_file.as_str())?;
            last_file = out_file;
        }
        writeln!(emitter.out)?;

        // Mark the last file as the default target.
        writeln!(emitter.out, "default {}", last_file)?;

        Ok(())
    }
}

pub struct Emitter {
    pub out: Box<dyn Write>,
    pub config_data: figment::Figment,
    pub global_config: config::GlobalConfig,
    pub workdir: Utf8PathBuf,
}

impl Emitter {
    fn new<T: Write + 'static>(
        out: T,
        config_data: figment::Figment,
        global_config: config::GlobalConfig,
        workdir: Utf8PathBuf,
    ) -> Self {
        Self {
            out: Box::new(out),
            config_data,
            global_config,
            workdir,
        }
    }

    /// Fetch a configuration value, or panic if it's missing.
    pub fn config_val(&self, key: &str) -> String {
        // TODO better error reporting here
        self.config_data
            .extract_inner::<String>(key)
            .expect("missing config key")
    }

    /// Fetch a configuration value, using a default if it's missing.
    pub fn config_or(&self, key: &str, default: &str) -> String {
        self.config_data
            .extract_inner::<String>(key)
            .unwrap_or_else(|_| default.into())
    }

    /// Emit a Ninja variable declaration for `name` based on the configured value for `key`.
    pub fn config_var(&mut self, name: &str, key: &str) -> std::io::Result<()> {
        self.var(name, &self.config_val(key))
    }

    /// Emit a Ninja variable declaration for `name` based on the configured value for `key`, or a
    /// default value if it's missing.
    pub fn config_var_or(&mut self, name: &str, key: &str, default: &str) -> std::io::Result<()> {
        self.var(name, &self.config_or(key, default))
    }

    /// Emit a Ninja variable declaration.
    pub fn var(&mut self, name: &str, value: &str) -> std::io::Result<()> {
        writeln!(self.out, "{} = {}", name, value)?;
        Ok(())
    }

    /// Emit a Ninja rule definition.
    pub fn rule(&mut self, name: &str, command: &str) -> std::io::Result<()> {
        writeln!(self.out, "rule {}", name)?;
        writeln!(self.out, "  command = {}", command)?;
        Ok(())
    }

    /// Emit a simple Ninja build command with one dependency.
    pub fn build(&mut self, rule: &str, input: &str, output: &str) -> std::io::Result<()> {
        self.build_cmd(output, rule, &[input], &[])
    }

    /// Emit a Ninja build command.
    pub fn build_cmd(
        &mut self,
        target: &str,
        rule: &str,
        deps: &[&str],
        implicit_deps: &[&str],
    ) -> std::io::Result<()> {
        write!(self.out, "build {}: {}", target, rule)?;
        for dep in deps {
            write!(self.out, " {}", dep)?;
        }
        if !implicit_deps.is_empty() {
            write!(self.out, " |")?;
            for dep in implicit_deps {
                write!(self.out, " {}", dep)?;
            }
        }
        writeln!(self.out)?;
        Ok(())
    }

    /// Emit a Ninja comment.
    pub fn comment(&mut self, text: &str) -> std::io::Result<()> {
        writeln!(self.out, "# {}", text)?;
        Ok(())
    }

    /// Add a file to the build directory.
    pub fn add_file(&self, name: &str, contents: &[u8]) -> std::io::Result<()> {
        let path = self.workdir.join(name);
        std::fs::write(path, contents)?;
        Ok(())
    }

    /// Get a path to an external file. The input `path` may be relative to our original
    /// invocation; we make it relative to the build directory so it can safely be used in the
    /// Ninja file.
    pub fn external_path(&self, path: &Utf8Path) -> Utf8PathBuf {
        relative_path(path, &self.workdir)
    }

    /// Add a variable parameter to a rule or build command.
    pub fn arg(&mut self, name: &str, value: &str) -> std::io::Result<()> {
        writeln!(self.out, "  {} = {}", name, value)?;
        Ok(())
    }
}
