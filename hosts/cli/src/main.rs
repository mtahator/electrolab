//! `ep` — the command line over the command registry (P1.M1.GEN.2).
//!
//! ```text
//! ep commands                               list every command
//! ep schemas <dir>                          write input/output JSON Schemas per command
//! ep call <command> ['<json>']              call any command with JSON input
//! ep import <file.pdf>...                   add to the library and read metadata
//! ep <engine> <verb> [--key value ...]      e.g. ep workspace add-file --path paper.pdf
//! ```
//! Engine commands run against `--project <folder>`, or the default library.

mod app;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use ep_bus::{CommandError, Envelope};
use serde_json::{Map, Value, json};

#[derive(Parser)]
#[command(
    name = "ep",
    version,
    about = "The ElectroPaper engines from the command line (JSON in, JSON out)"
)]
struct Cli {
    /// Library or project folder to use (default: the default library).
    #[arg(long, global = true)]
    project: Option<PathBuf>,
    /// Log level (`EP_LOG` overrides).
    #[arg(long, global = true, default_value = "warn")]
    log: String,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// List every command with its permission and stability.
    Commands,
    /// Write one JSON Schema file per command (input and output) into a folder.
    Schemas {
        /// Output folder.
        dir: PathBuf,
    },
    /// Call a command with JSON input.
    Call {
        /// Command name, `engine.verb`.
        command: String,
        /// JSON input (default `null`).
        input: Option<String>,
    },
    /// Add PDFs to the library and read their metadata.
    Import {
        /// Files.
        #[arg(required = true)]
        files: Vec<PathBuf>,
    },
    /// `ep <engine> <verb> [--key value ...]`.
    #[command(external_subcommand)]
    Engine(Vec<String>),
}

/// `--key value` pairs into a JSON object; values are JSON when they parse, else strings.
fn flags_to_json(args: &[String]) -> anyhow::Result<Value> {
    let mut map = Map::new();
    let mut it = args.iter();
    while let Some(k) = it.next() {
        let key = k
            .strip_prefix("--")
            .ok_or_else(|| anyhow::anyhow!("expected --key, found {k}"))?;
        let v = it
            .next()
            .ok_or_else(|| anyhow::anyhow!("--{key} needs a value"))?;
        let value = serde_json::from_str(v).unwrap_or_else(|_| Value::String(v.clone()));
        map.insert(key.replace('-', "_"), value);
    }
    Ok(if map.is_empty() {
        Value::Null
    } else {
        Value::Object(map)
    })
}

fn print(v: &Value) {
    println!("{}", serde_json::to_string_pretty(v).unwrap_or_default());
}

fn report(e: &CommandError) -> ExitCode {
    eprintln!(
        "{}",
        serde_json::to_string_pretty(&e).unwrap_or_else(|_| e.to_string())
    );
    ExitCode::from(2)
}

async fn run(cli: Cli) -> anyhow::Result<ExitCode> {
    let app = app::App::build()?;
    match cli.cmd {
        Cmd::Commands => {
            let list: Vec<Value> =
                app.bus.commands().into_iter().map(|c| json!({ "name": c.name, "summary": c.summary, "stability": c.stability, "permission": c.permission.operation, "streaming": c.streaming })).collect();
            print(&Value::Array(list));
        }
        Cmd::Schemas { dir } => {
            let n = app::write_schemas(&app.bus, &dir)?;
            eprintln!("wrote {n} command schemas to {}", dir.display());
        }
        Cmd::Call { command, input } => {
            let input: Value = input
                .as_deref()
                .map(serde_json::from_str)
                .transpose()?
                .unwrap_or(Value::Null);
            if let Err(e) = app.open(cli.project.as_deref(), &command).await {
                return Ok(report(&e));
            }
            match app.bus.call(Envelope::new(command, input)).await {
                Ok(v) => print(&v),
                Err(e) => return Ok(report(&e)),
            }
        }
        Cmd::Import { files } => {
            if let Err(e) = app.open(cli.project.as_deref(), "workspace.add_file").await {
                return Ok(report(&e));
            }
            let mut out = vec![];
            for f in files {
                match app.import(&f).await {
                    Ok(p) => out.push(serde_json::to_value(p)?),
                    Err(e) => return Ok(report(&e)),
                }
            }
            print(&Value::Array(out));
        }
        Cmd::Engine(args) => {
            let [engine, verb, rest @ ..] = args.as_slice() else {
                anyhow::bail!("usage: ep <engine> <verb> [--key value ...]");
            };
            let command = format!("{engine}.{}", verb.replace('-', "_"));
            let mut input = flags_to_json(rest)?;
            // No flags for a command whose input is an object (all fields optional): send `{}`.
            let takes_object = app.bus.commands().iter().any(|c| {
                c.name == command
                    && c.input_schema.get("type").and_then(Value::as_str) == Some("object")
            });
            if input.is_null() && takes_object {
                input = json!({});
            }
            if let Err(e) = app.open(cli.project.as_deref(), &command).await {
                return Ok(report(&e));
            }
            match app.bus.call(Envelope::new(command, input)).await {
                Ok(v) => print(&v),
                Err(e) => return Ok(report(&e)),
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let _ = ep_telemetry::init(ep_telemetry::Output::Console, &cli.log);
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    match rt.block_on(run(cli)) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_become_json() {
        let v = flags_to_json(&[
            "--path".into(),
            "a b.pdf".into(),
            "--limit".into(),
            "5".into(),
            "--dry-run".into(),
            "true".into(),
        ])
        .expect("json");
        assert_eq!(v, json!({ "path": "a b.pdf", "limit": 5, "dry_run": true }));
        assert_eq!(flags_to_json(&[]).expect("empty"), Value::Null);
        assert!(flags_to_json(&["path".into()]).is_err());
    }
}
