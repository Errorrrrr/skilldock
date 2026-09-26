use clap::{Parser, Subcommand};
use serde_json::{Value, json};
use skilldock_core::Engine;
use std::path::PathBuf;
mod install;

#[derive(Parser)]
#[command(
    name = "skilldock",
    version,
    about = "统一管理、安装和软链分发 Agent Skills"
)]
struct Args {
    #[arg(
        long,
        global = true,
        help = "隔离配置目录，供独立配置实例或临时验证使用"
    )]
    config_dir: Option<PathBuf>,
    #[arg(long, global = true, help = "输出机器可读 JSON")]
    json: bool,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    Init {
        path: String,
    },
    List,
    Scan {
        path: String,
    },
    Discover,
    Import {
        path: String,
        #[arg(long)]
        adopt: bool,
        #[arg(long = "select")]
        selected: Vec<String>,
    },
    Git {
        url: String,
        #[arg(long, default_value = "HEAD")]
        reference: String,
        #[arg(long, default_value = "")]
        subdir: String,
    },
    Search {
        query: String,
        #[arg(long = "site")]
        sites: Vec<String>,
    },
    #[command(about = "从网站、Git 或本地目录入库，并分发到指定工具")]
    Install(install::InstallArgs),
    Target {
        name: String,
        path: String,
        #[arg(long, default_value = "custom")]
        tool: String,
        #[arg(long, default_value = "project")]
        scope: String,
    },
    Plan {
        #[arg(long = "skill", required = true)]
        skills: Vec<String>,
        #[arg(long = "target", required = true)]
        targets: Vec<String>,
    },
    Distribute {
        #[arg(long = "skill", required = true)]
        skills: Vec<String>,
        #[arg(long = "target", required = true)]
        targets: Vec<String>,
        #[arg(long)]
        revision: u32,
    },
    Revoke {
        #[arg(long = "binding", required = true)]
        bindings: Vec<String>,
    },
    Diagnose,
    Recover {
        task_id: String,
    },
    Update {
        source_id: String,
        #[arg(long)]
        apply: bool,
    },
    Migrate {
        path: String,
    },
    #[command(about = "执行完整 API 请求，覆盖预设、策略和导入导出等操作")]
    Exec {
        request: String,
    },
}
#[tokio::main]
async fn main() {
    let args = Args::parse();
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter("warn")
        .init();
    if let Command::Install(input) = &args.command {
        let report = install::run(args.config_dir.clone(), input).await;
        println!(
            "{}",
            if args.json {
                serde_json::to_string(&report).unwrap()
            } else {
                serde_json::to_string_pretty(&report).unwrap()
            }
        );
        if report["status"] != "succeeded" {
            std::process::exit(1);
        }
        return;
    }
    let request = match args.command {
        Command::Init { path } => json!({"action":"configure","path":path}),
        Command::List => json!({"action":"snapshot"}),
        Command::Scan { path } => json!({"action":"scan","path":path}),
        Command::Discover => json!({"action":"discover"}),
        Command::Import {
            path,
            adopt,
            selected,
        } => json!({"action":"import_folder","path":path,"adopt":adopt,"selectedPaths":selected}),
        Command::Git {
            url,
            reference,
            subdir,
        } => json!({"action":"import_git","url":url,"reference":reference,"subdir":subdir}),
        Command::Search { query, sites } => {
            json!({"action":"search_catalog","query":query,"sites":sites})
        }
        Command::Install(_) => unreachable!(),
        Command::Target {
            name,
            path,
            tool,
            scope,
        } => json!({"action":"add_target","name":name,"path":path,"tool":tool,"scope":scope}),
        Command::Plan { skills, targets } => {
            json!({"action":"plan","skillIds":skills,"targetIds":targets})
        }
        Command::Distribute {
            skills,
            targets,
            revision,
        } => {
            json!({"action":"distribute","skillIds":skills,"targetIds":targets,"expectedRevision":revision})
        }
        Command::Revoke { bindings } => json!({"action":"revoke","bindingIds":bindings}),
        Command::Diagnose => json!({"action":"diagnose"}),
        Command::Recover { task_id } => json!({"action":"recover","taskId":task_id}),
        Command::Update { source_id, apply } => {
            json!({"action":"check_source","sourceId":source_id,"apply":apply})
        }
        Command::Migrate { path } => json!({"action":"migrate_storage","path":path}),
        Command::Exec { request } => match serde_json::from_str::<Value>(&request) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("无效 JSON：{e}");
                std::process::exit(2)
            }
        },
    };
    let result = match Engine::new(args.config_dir) {
        Ok(engine) => engine.execute(request).await,
        Err(e) => Err(e),
    };
    match result {
        Ok(value) => {
            if args.json {
                println!("{}", serde_json::to_string(&value).unwrap());
            } else {
                println!("{}", serde_json::to_string_pretty(&value).unwrap());
            }
        }
        Err(error) => {
            if args.json {
                eprintln!("{}", json!({"error":error.to_string()}));
            } else {
                eprintln!("{error}");
            }
            std::process::exit(1);
        }
    }
}
