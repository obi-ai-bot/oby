fn main() {
    let args: Result<Vec<String>, _> = std::env::args_os()
        .skip(1)
        .map(|a| a.into_string())
        .collect();
    let code = match args {
        Ok(args) => oby::main(&args),
        Err(_) => {
            eprint!("oby: an argument isn't UTF-8 text\n{}", oby::args::USAGE);
            oby::exit::Exit::Usage as i32
        }
    };
    std::process::exit(code);
}
