mod bootstrap;
mod cli;

fn main() -> Result<(), bootstrap::BootstrapError> {
    let app = bootstrap::build()?;
    app.run()?;
    Ok(())
}
