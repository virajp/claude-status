use std::process::ExitCode;

fn main() -> ExitCode {
    ExitCode::from(claude_status::run() as u8)
}
