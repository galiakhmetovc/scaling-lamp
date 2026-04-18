use agent_persistence::PersistenceScaffold;
use agent_runtime::RuntimeScaffold;

fn main() {
    let persistence = PersistenceScaffold::default();
    let runtime = RuntimeScaffold::default();

    println!(
        "agentd scaffold ready: data_dir={} components={}",
        persistence.config.data_dir.display(),
        runtime.component_count()
    );
}
