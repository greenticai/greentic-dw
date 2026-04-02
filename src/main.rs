fn main() {
    if let Err(error) = greentic_dw_cli::run_from_env() {
        greentic_dw_cli::print_error(&error);
        std::process::exit(1);
    }
}
