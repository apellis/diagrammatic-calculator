fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--version") | Some("version") => println!("diagcalc {}", diagcalc_core::VERSION),
        _ => {
            eprintln!("usage: diagcalc <command>  (commands arrive in phase 1)");
            std::process::exit(2);
        }
    }
}
