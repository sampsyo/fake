use crate::config;
use crate::driver::{Driver, OpRef, Plan, SetupRef, StateRef};
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::Path;
use std::process::Command;

pub struct Run<'a> {
    pub driver: &'a Driver,
    pub plan: Plan,
    pub config: config::Config,
}

impl<'a> Run<'a> {
    pub fn new(driver: &'a Driver, plan: Plan) -> Self {
        Self {
            driver,
            plan,
            config: config::Config::new().expect("failed to load config"),
        }
    }

    /// Just print the plan for debugging purposes.
    pub fn show(self) {
        println!("start: {}", self.plan.start.display());
        for (op, file) in &self.plan.steps {
            println!(
                "{}: {} -> {}",
                op,
                self.driver.ops[*op].name,
                file.display()
            );
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
        states.insert(
            self.driver.ops[first_op].input,
            self.plan.start.to_string_lossy().to_string(),
        );
        for (op, file) in &self.plan.steps {
            states.insert(
                self.driver.ops[*op].output,
                file.to_string_lossy().to_string(),
            );
            ops.insert(*op);
        }

        // Show all states.
        for (state_ref, state) in self.driver.states.iter() {
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
    pub fn emit_to_dir(self, dir: &Path) -> Result<(), std::io::Error> {
        std::fs::create_dir_all(dir)?;
        let ninja_path = dir.join("build.ninja");
        let ninja_file = std::fs::File::create(ninja_path)?;

        self.emit(ninja_file)
    }

    /// Emit `build.ninja` to a temporary directory and then actually execute ninja.
    pub fn emit_and_run(self, dir: &Path) -> Result<(), std::io::Error> {
        // TODO: This workaround for lifetime stuff in the config isn't great.
        let keep = self.config.global.keep_build_dir;
        let ninja = self.config.global.ninja.clone();

        let stale_dir = dir.exists();
        self.emit_to_dir(dir)?;

        // Run `ninja` in the working directory.
        Command::new(ninja).current_dir(dir).status()?;

        // Remove the temporary directory unless it already existed at the start *or* the user specified `--keep`.
        if !keep && !stale_dir {
            std::fs::remove_dir_all(dir)?;
        }

        Ok(())
    }

    fn emit<T: Write + 'static>(self, out: T) -> Result<(), std::io::Error> {
        let mut emitter = Emitter::new(out, self.config.data);

        // Emit the setup for each operation used in the plan, only once.
        let mut done_setups = HashSet::<SetupRef>::new();
        for (op, _) in &self.plan.steps {
            if let Some(setup) = self.driver.ops[*op].setup {
                if done_setups.insert(setup) {
                    let setup = &self.driver.setups[setup];
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
            op.emit.build(&mut emitter, &last_file, &out_file)?;
            last_file = out_file;
        }
        writeln!(emitter.out)?;

        // Mark the last file as the default target.
        write!(emitter.out, "default ")?;
        emitter.filename(&last_file)?;
        writeln!(emitter.out)?;

        Ok(())
    }
}

pub struct Emitter {
    pub out: Box<dyn Write>,
    pub config: figment::Figment,
}

impl Emitter {
    fn new<T: Write + 'static>(out: T, config: figment::Figment) -> Self {
        Self {
            out: Box::new(out),
            config,
        }
    }

    /// Fetch a configuration value, or panic if it's missing.
    pub fn config_val(&self, key: &str) -> String {
        // TODO better error reporting here
        self.config
            .extract_inner::<String>(key)
            .expect("missing config key")
    }

    /// Fetch a configuration value, using a default if it's missing.
    pub fn config_or(&self, key: &str, default: &str) -> String {
        self.config
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

    /// Emit a Ninja build command.
    pub fn build(&mut self, rule: &str, input: &Path, output: &Path) -> std::io::Result<()> {
        self.out.write_all(b"build ")?;
        self.filename(output)?;
        write!(self.out, ": {} ", rule)?;
        self.filename(input)?;
        self.out.write_all(b"\n")?;
        Ok(())
    }

    /// Emit a Ninja comment.
    pub fn comment(&mut self, text: &str) -> std::io::Result<()> {
        writeln!(self.out, "# {}", text)?;
        Ok(())
    }

    /// Write a filename to the Ninja file.
    pub fn filename(&mut self, path: &Path) -> std::io::Result<()> {
        // This seems like the best/only way to portably preserve the raw filename??
        self.out.write_all(path.as_os_str().as_encoded_bytes())?;
        Ok(())
    }
}
