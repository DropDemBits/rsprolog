extern crate getopts;

use getopts::Options;
use std::env;

fn show_usage(program_name: &str, opts: &Options) {
    let brief = format!("Usage: {} [options] [main file]", program_name);
    print!("{}", opts.usage(&brief));
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let program = args[0].clone();

    let mut opts = Options::new();
    opts.long_only(true);
    opts.optopt(
        "b",
        "build",
        "Compile a Turing program from a source file",
        "PATH",
    );
    opts.optopt(
        "r",
        "rebuild",
        "Rebuilds a compiled Turing program from a bytecode file or executable",
        "PATH",
    );
    opts.optmulti(
        "",
        "dump",
        "Dumps the specified structure\n('KIND' can 'ast', 'scope', or 'types')",
        "KIND",
    );
    opts.optflag(
        "",
        "only_parser",
        "Only runs the parser stage. Used for testing",
    );
    opts.optflag("M", "mute_warnings", "Mutes all warnings");
    opts.optflag("", "help", "Shows this help message");

    let matches = match opts.parse(&args[1..]) {
        Ok(m) => m,
        Err(f) => {
            eprintln!("{}", f.to_string());
            show_usage(&program, &opts);
            return;
        }
    };

    if matches.opt_present("help") {
        // Show the help
        show_usage(&program, &opts);
        return;
    }

    if let Some(source_path) = matches.opt_str("build") {
        let dump_out = matches.opt_strs("dump");
        let mute_warnings = matches.opt_present("mute_warnings");
        let only_parser = matches.opt_present("only_parser");

        if !toc::compile_file(&source_path, dump_out, mute_warnings, only_parser) {
            // Exit with a non-zero status
            std::process::exit(-1);
        }
    } else if let Some(bytecode_path) = matches.opt_str("rebuild") {
        todo!("Recompiling file {} (Not Supported Yet)", bytecode_path);
    } else {
        show_usage(&program, &opts);
    }
}
