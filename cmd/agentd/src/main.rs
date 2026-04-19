mod bootstrap;
mod cli;
mod execution;

fn main() -> Result<(), bootstrap::BootstrapError> {
    let app = bootstrap::build()?;
    app.run()?;
    Ok(())
}
