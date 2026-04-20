use super::{
    ExecStartInput, FsGlobInput, FsListInput, FsPatchEdit, FsPatchInput, FsReadInput,
    FsSearchInput, FsWriteInput, ProcessKillInput, ProcessResultStatus, ProcessWaitInput, ToolCall,
    ToolCatalog, ToolFamily, ToolName, ToolRuntime, WebFetchInput, WebSearchInput, WebToolClient,
};
use crate::workspace::WorkspaceRef;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

#[test]
fn catalog_exposes_distinct_families_and_policy_flags() {
    let catalog = ToolCatalog::default();
    let artifact_read = catalog
        .definition(ToolName::ArtifactRead)
        .expect("artifact_read");
    let artifact_search = catalog
        .definition(ToolName::ArtifactSearch)
        .expect("artifact_search");
    let exec_start = catalog.definition(ToolName::ExecStart).expect("exec_start");
    let fs_glob = catalog.definition(ToolName::FsGlob).expect("fs_glob");
    let fs_patch = catalog.definition(ToolName::FsPatch).expect("fs_patch");
    let plan_read = catalog.definition(ToolName::PlanRead).expect("plan_read");
    let plan_write = catalog.definition(ToolName::PlanWrite).expect("plan_write");
    let web_fetch = catalog.definition(ToolName::WebFetch).expect("web_fetch");
    let web_search = catalog.definition(ToolName::WebSearch).expect("web_search");
    let fs_read = catalog.definition(ToolName::FsRead).expect("fs_read");
    let fs_write = catalog.definition(ToolName::FsWrite).expect("fs_write");

    assert_eq!(catalog.families, ["fs", "web", "exec", "plan", "offload"]);
    assert_eq!(artifact_read.family, ToolFamily::Offload);
    assert_eq!(artifact_search.family, ToolFamily::Offload);
    assert_eq!(exec_start.family, ToolFamily::Exec);
    assert_eq!(fs_glob.family, ToolFamily::Filesystem);
    assert_eq!(fs_patch.family, ToolFamily::Filesystem);
    assert_eq!(plan_read.family, ToolFamily::Planning);
    assert_eq!(plan_write.family, ToolFamily::Planning);
    assert_eq!(web_fetch.family, ToolFamily::Web);
    assert_eq!(web_search.family, ToolFamily::Web);
    assert!(artifact_read.policy.read_only);
    assert!(artifact_search.policy.read_only);
    assert!(exec_start.policy.requires_approval);
    assert!(fs_glob.policy.read_only);
    assert!(fs_patch.policy.destructive);
    assert!(plan_read.policy.read_only);
    assert!(!plan_write.policy.read_only);
    assert!(!plan_write.policy.requires_approval);
    assert!(web_fetch.policy.read_only);
    assert!(web_search.policy.read_only);
    assert!(fs_read.policy.read_only);
    assert!(fs_write.policy.destructive);
}

#[test]
fn filesystem_tools_read_write_list_and_search_within_workspace() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = WorkspaceRef::new(temp.path());
    let mut runtime = ToolRuntime::new(workspace.clone());

    runtime
        .invoke(ToolCall::FsWrite(FsWriteInput {
            path: "docs/notes.txt".to_string(),
            content: "alpha\nbeta\n".to_string(),
        }))
        .expect("fs_write");
    runtime
        .invoke(ToolCall::FsWrite(FsWriteInput {
            path: "docs/summary.txt".to_string(),
            content: "beta\ngamma\n".to_string(),
        }))
        .expect("fs_write summary");

    let read = runtime
        .invoke(ToolCall::FsRead(FsReadInput {
            path: "docs/notes.txt".to_string(),
        }))
        .expect("fs_read")
        .into_fs_read()
        .expect("fs_read output");
    let list = runtime
        .invoke(ToolCall::FsList(FsListInput {
            path: "docs".to_string(),
            recursive: true,
        }))
        .expect("fs_list")
        .into_fs_list()
        .expect("fs_list output");
    let search = runtime
        .invoke(ToolCall::FsSearch(FsSearchInput {
            path: "docs".to_string(),
            query: "beta".to_string(),
        }))
        .expect("fs_search")
        .into_fs_search()
        .expect("fs_search output");

    assert_eq!(read.path, "docs/notes.txt");
    assert_eq!(read.content, "alpha\nbeta\n");
    assert_eq!(
        list.entries
            .iter()
            .filter(|entry| entry.kind == crate::workspace::WorkspaceEntryKind::File)
            .map(|entry| entry.path.as_str())
            .collect::<Vec<_>>(),
        vec!["docs/notes.txt", "docs/summary.txt"]
    );
    assert_eq!(search.matches.len(), 2);
    assert_eq!(search.matches[0].path, "docs/notes.txt");
    assert_eq!(search.matches[1].path, "docs/summary.txt");
}

#[test]
fn filesystem_tools_reject_paths_that_escape_workspace() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = WorkspaceRef::new(temp.path());
    let mut runtime = ToolRuntime::new(workspace);

    assert!(
        runtime
            .invoke(ToolCall::FsRead(FsReadInput {
                path: "../secret.txt".to_string(),
            }))
            .is_err()
    );
}

#[test]
fn filesystem_tools_glob_and_patch_files_with_exact_edits() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = WorkspaceRef::new(temp.path());
    let mut runtime = ToolRuntime::new(workspace.clone());

    runtime
        .invoke(ToolCall::FsWrite(FsWriteInput {
            path: "src/main.rs".to_string(),
            content: "fn main() {\n    println!(\"old\");\n}\n".to_string(),
        }))
        .expect("fs_write main");
    runtime
        .invoke(ToolCall::FsWrite(FsWriteInput {
            path: "src/lib.rs".to_string(),
            content: "pub fn helper() {}\n".to_string(),
        }))
        .expect("fs_write lib");

    let globbed = runtime
        .invoke(ToolCall::FsGlob(FsGlobInput {
            path: "src".to_string(),
            pattern: "**/*.rs".to_string(),
        }))
        .expect("fs_glob")
        .into_fs_glob()
        .expect("fs_glob output");
    let patched = runtime
        .invoke(ToolCall::FsPatch(FsPatchInput {
            path: "src/main.rs".to_string(),
            edits: vec![FsPatchEdit {
                old: "println!(\"old\");".to_string(),
                new: "println!(\"new\");".to_string(),
                replace_all: false,
            }],
        }))
        .expect("fs_patch");
    let read = runtime
        .invoke(ToolCall::FsRead(FsReadInput {
            path: "src/main.rs".to_string(),
        }))
        .expect("fs_read patched")
        .into_fs_read()
        .expect("fs_read output");

    assert_eq!(
        globbed
            .entries
            .iter()
            .filter(|entry| entry.kind == crate::workspace::WorkspaceEntryKind::File)
            .map(|entry| entry.path.as_str())
            .collect::<Vec<_>>(),
        vec!["src/lib.rs", "src/main.rs"]
    );
    assert_eq!(patched.summary(), "fs_patch path=src/main.rs edits=1");
    assert!(read.content.contains("println!(\"new\");"));
}

#[test]
fn fs_patch_rejects_ambiguous_single_replace_edits() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = WorkspaceRef::new(temp.path());
    let mut runtime = ToolRuntime::new(workspace);

    runtime
        .invoke(ToolCall::FsWrite(FsWriteInput {
            path: "docs/repeated.txt".to_string(),
            content: "same\nsame\n".to_string(),
        }))
        .expect("fs_write repeated");

    assert!(
        runtime
            .invoke(ToolCall::FsPatch(FsPatchInput {
                path: "docs/repeated.txt".to_string(),
                edits: vec![FsPatchEdit {
                    old: "same".to_string(),
                    new: "updated".to_string(),
                    replace_all: false,
                }],
            }))
            .is_err()
    );
}

#[test]
fn structured_exec_treats_shell_tokens_as_literal_args() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = WorkspaceRef::new(temp.path());
    let mut runtime = ToolRuntime::new(workspace);

    let started = runtime
        .invoke(ToolCall::ExecStart(ExecStartInput {
            executable: "/bin/echo".to_string(),
            args: vec!["left|right".to_string()],
            cwd: None,
        }))
        .expect("exec_start")
        .into_process_start()
        .expect("process start");
    let waited = runtime
        .invoke(ToolCall::ExecWait(ProcessWaitInput {
            process_id: started.process_id.clone(),
        }))
        .expect("exec_wait")
        .into_process_result()
        .expect("process result");

    assert_eq!(waited.status, ProcessResultStatus::Exited);
    assert_eq!(waited.exit_code, Some(0));
    assert_eq!(waited.stdout, "left|right\n");
}

#[test]
fn exec_kill_terminates_structured_processes() {
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = WorkspaceRef::new(temp.path());
    let mut runtime = ToolRuntime::new(workspace);

    let exec_started = runtime
        .invoke(ToolCall::ExecStart(ExecStartInput {
            executable: "/bin/sleep".to_string(),
            args: vec!["5".to_string()],
            cwd: None,
        }))
        .expect("exec_start sleep")
        .into_process_start()
        .expect("sleep start");
    let killed = runtime
        .invoke(ToolCall::ExecKill(ProcessKillInput {
            process_id: exec_started.process_id,
        }))
        .expect("exec_kill")
        .into_process_result()
        .expect("killed process result");

    assert_eq!(killed.status, ProcessResultStatus::Killed);
}

#[test]
fn web_tools_fetch_pages_and_return_search_results() {
    let server = TestHttpServer::spawn();
    let temp = tempfile::tempdir().expect("tempdir");
    let workspace = WorkspaceRef::new(temp.path());
    let mut runtime = ToolRuntime::with_web_client(
        workspace,
        WebToolClient::for_tests(server.base_url(), server.search_url()),
    );

    let fetched = runtime
        .invoke(ToolCall::WebFetch(WebFetchInput {
            url: server.page_url(),
        }))
        .expect("web_fetch")
        .into_web_fetch()
        .expect("web_fetch output");
    let searched = runtime
        .invoke(ToolCall::WebSearch(WebSearchInput {
            query: "agent runtime".to_string(),
            limit: 5,
        }))
        .expect("web_search")
        .into_web_search()
        .expect("web_search output");

    assert_eq!(fetched.url, server.page_url());
    assert_eq!(fetched.status_code, 200);
    assert!(fetched.body.contains("Agent runtime page"));
    assert_eq!(searched.results.len(), 2);
    assert_eq!(searched.results[0].title, "Agent runtime docs");
    assert_eq!(searched.results[0].url, "https://example.test/docs");
}

struct TestHttpServer {
    base_url: String,
    search_url: String,
}

impl TestHttpServer {
    fn spawn() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
        let address = listener.local_addr().expect("local addr");
        let base_url = format!("http://{}", address);
        let search_url = format!("{}/search", base_url);

        thread::spawn(move || {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().expect("accept");
                let mut buffer = [0_u8; 4096];
                let bytes = stream.read(&mut buffer).expect("read request");
                let request = String::from_utf8_lossy(&buffer[..bytes]);
                let path = request
                    .lines()
                    .next()
                    .and_then(|line| line.split_whitespace().nth(1))
                    .unwrap_or("/");

                let body = if path.starts_with("/search") {
                    "<html><body>\
                     <a class=\"result__a\" href=\"https://example.test/docs\">Agent runtime docs</a>\
                     <a class=\"result__snippet\">Typed tools and run engine</a>\
                     <a class=\"result__a\" href=\"https://example.test/blog\">Blog post</a>\
                     <a class=\"result__snippet\">Web tool coverage</a>\
                     </body></html>"
                } else {
                    "<html><head><title>Agent runtime page</title></head>\
                     <body>Agent runtime page body</body></html>"
                };

                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                )
                .expect("write response");
            }
        });

        Self {
            base_url,
            search_url,
        }
    }

    fn base_url(&self) -> &str {
        &self.base_url
    }

    fn search_url(&self) -> &str {
        &self.search_url
    }

    fn page_url(&self) -> String {
        format!("{}/page", self.base_url)
    }
}
