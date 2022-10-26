use clap::Parser;
use cp_sandbox::SandboxBuilder;
use std::time::Duration;

#[derive(Parser)]
struct Args {
    #[arg(long, short)]
    command: String,

    #[arg(long, short)]
    root: String,

    #[arg(long, short)]
    upper: String,

    #[arg(long, short)]
    args: Vec<String>,

    #[arg(long, short)]
    pids: Option<i64>,

    #[arg(long, short)]
    memory: Option<i64>,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let builder = SandboxBuilder::new(&args.command, &args.root, &args.upper).args(&args.args);

    let builder = if let Some(m) = args.memory {
        builder.memory(m)
    } else {
        builder
    };

    let builder = builder.pids(args.pids);

    let mut sand = builder.build();

    let res = sand.run_with_timeout(Duration::from_secs(5)).await.unwrap();

    println!("{:?}", res);
}
