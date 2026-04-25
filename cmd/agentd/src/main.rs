use agentd::bootstrap;

fn main() -> Result<(), bootstrap::BootstrapError> {
    let app = bootstrap::build_for_args(std::env::args().skip(1))?;
    app.run()?;
    Ok(())
}
