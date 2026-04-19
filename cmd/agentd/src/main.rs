use agentd::bootstrap;

fn main() -> Result<(), bootstrap::BootstrapError> {
    let app = bootstrap::build()?;
    app.run()?;
    Ok(())
}
