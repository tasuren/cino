use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use serde_json::Value as JsonValue;

use cino_ir::lower_program;
use cino_sema::analyze;
use cino_syntax::parse_program;
use cino_vm::{IrVmProgram, VmLimits, VmProgram, VmValue};

#[derive(Parser)]
#[command(name = "cino")]
#[command(about = "cino MVP CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Syntax and static analysis check
    Check {
        /// Input source file
        #[arg(short, long)]
        file: PathBuf,
    },
    /// Execute update/query
    Run {
        #[command(subcommand)]
        subcommand: RunCommands,
    },
    /// Generate documentation from source
    Docgen {
        /// Input source file
        #[arg(short, long)]
        file: PathBuf,
        /// Language (ja/en)
        #[arg(short, long, default_value = "ja")]
        lang: String,
        /// Output directory
        #[arg(short, long)]
        out: PathBuf,
    },
}

#[derive(Subcommand)]
enum RunCommands {
    /// Run update function
    Update {
        /// Input source file
        #[arg(short, long)]
        file: PathBuf,
        /// Initial state (JSON)
        #[arg(short, long)]
        state: String,
        /// Event payload (JSON)
        #[arg(short, long)]
        event: String,
    },
    /// Run query function
    Query {
        /// Input source file
        #[arg(short, long)]
        file: PathBuf,
        /// Current state (JSON)
        #[arg(short, long)]
        state: String,
        /// Query payload (JSON)
        #[arg(short, long)]
        query: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Check { file } => {
            let source = fs::read_to_string(&file)
                .with_context(|| format!("Failed to read file: {:?}", file))?;
            let program = parse_program(&source)
                .map_err(|e| anyhow!("Parse error: {} at {}:{}", e.message, e.position.line, e.position.column))?;
            
            let analysis = analyze(&program);
            if !analysis.is_ok() {
                for diag in analysis.diagnostics {
                    eprintln!("{}:{}:{} [{}]: {}", file.display(), diag.line, diag.column, diag.code, diag.message);
                }
                std::process::exit(1);
            }
            println!("Check passed!");
        }
        Commands::Run { subcommand } => match subcommand {
            RunCommands::Update { file, state, event } => {
                let vm = setup_vm(&file)?;
                let state_val = json_to_vm_value(&serde_json::from_str(&state)?)?;
                let event_val = json_to_vm_value(&serde_json::from_str(&event)?)?;
                
                let (next_state, actions) = vm.update(&state_val, &event_val, &VmLimits::default())
                    .map_err(|e| anyhow!("VM Error: {}", e))?;
                
                let output = serde_json::json!({
                    "next_state": vm_value_to_json(&next_state),
                    "actions": actions.into_iter().map(|a| vm_value_to_json(&a)).collect::<Vec<_>>()
                });
                println!("{}", serde_json::to_string_pretty(&output)?);
            }
            RunCommands::Query { file, state, query } => {
                let vm = setup_vm(&file)?;
                let state_val = json_to_vm_value(&serde_json::from_str(&state)?)?;
                let query_val = json_to_vm_value(&serde_json::from_str(&query)?)?;
                
                let result = vm.query(&state_val, &query_val, &VmLimits::default())
                    .map_err(|e| anyhow!("VM Error: {}", e))?;
                
                println!("{}", serde_json::to_string_pretty(&vm_value_to_json(&result))?);
            }
        },
        Commands::Docgen { file, lang: _, out } => {
            let source = fs::read_to_string(&file)?;
            let program = parse_program(&source)
                .map_err(|e| anyhow!("Parse error: {}", e.message))?;
            
            fs::create_dir_all(&out)?;
            let mut md = String::new();
            md.push_str("# cino Specification\n\n");
            
            md.push_str("## Top-level Declarations\n\n");
            for decl in &program.decls {
                match decl {
                    cino_syntax::TopDecl::Type(td) => {
                        md.push_str(&format!("- **Type**: {}\n", td.name));
                    }
                    cino_syntax::TopDecl::Function(fd) => {
                        md.push_str(&format!("- **Function**: {} ({:?})\n", fd.name, fd.kind));
                    }
                }
            }
            
            let out_file = out.join("spec.md");
            fs::write(&out_file, md)?;
            println!("Documentation generated to {:?}", out_file);
        }
    }

    Ok(())
}

fn setup_vm(file: &PathBuf) -> Result<IrVmProgram> {
    let source = fs::read_to_string(file)
        .with_context(|| format!("Failed to read file: {:?}", file))?;
    let program = parse_program(&source)
        .map_err(|e| anyhow!("Parse error: {} at {}:{}", e.message, e.position.line, e.position.column))?;
    
    let lowered = lower_program(&program);
    if !lowered.diagnostics.is_empty() {
        for diag in lowered.diagnostics {
            eprintln!("Lowering Diagnostic [{}]: {}", diag.code, diag.message);
        }
    }
    
    let ir = lowered.program.ok_or_else(|| anyhow!("Failed to lower program to IR"))?;
    IrVmProgram::from_ir(ir).map_err(|e| anyhow!("VM Program Error: {}", e))
}

fn json_to_vm_value(json: &JsonValue) -> Result<VmValue> {
    match json {
        JsonValue::Null => Ok(VmValue::Unit),
        JsonValue::Bool(b) => Ok(VmValue::Bool(*b)),
        JsonValue::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(VmValue::Int(i))
            } else {
                Err(anyhow!("Invalid number: only i64 is supported for now"))
            }
        }
        JsonValue::String(s) => Ok(VmValue::String(s.clone())),
        JsonValue::Array(arr) => {
            let mut values = Vec::new();
            for item in arr {
                values.push(json_to_vm_value(item)?);
            }
            Ok(VmValue::List(values))
        }
        JsonValue::Object(obj) => {
            // Check for special tags like $tuple or $tag/$fields (parity with cino-codec)
            if obj.len() == 1 && obj.contains_key("$tuple") {
                let items = obj.get("$tuple").unwrap().as_array().ok_or_else(|| anyhow!("$tuple must be an array"))?;
                let mut values = Vec::new();
                for item in items {
                    values.push(json_to_vm_value(item)?);
                }
                return Ok(VmValue::Tuple(values));
            }

            if obj.contains_key("$tag") && obj.contains_key("$fields") {
                let tag = obj.get("$tag").unwrap().as_str().ok_or_else(|| anyhow!("$tag must be a string"))?.to_string();
                let fields_obj = obj.get("$fields").unwrap().as_object().ok_or_else(|| anyhow!("$fields must be an object"))?;
                let mut fields = BTreeMap::new();
                for (k, v) in fields_obj {
                    fields.insert(k.clone(), json_to_vm_value(v)?);
                }
                return Ok(VmValue::Enum { tag, fields });
            }

            let mut map = BTreeMap::new();
            for (k, v) in obj {
                map.insert(k.clone(), json_to_vm_value(v)?);
            }
            Ok(VmValue::Map(map))
        }
    }
}

fn vm_value_to_json(value: &VmValue) -> JsonValue {
    match value {
        VmValue::Unit => JsonValue::Null,
        VmValue::Int(i) => JsonValue::Number((*i).into()),
        VmValue::Bool(b) => JsonValue::Bool(*b),
        VmValue::String(s) => JsonValue::String(s.clone()),
        VmValue::List(items) => JsonValue::Array(items.iter().map(vm_value_to_json).collect()),
        VmValue::Tuple(items) => {
            let mut obj = serde_json::Map::new();
            obj.insert("$tuple".to_string(), JsonValue::Array(items.iter().map(vm_value_to_json).collect()));
            JsonValue::Object(obj)
        }
        VmValue::Map(entries) => {
            let mut obj = serde_json::Map::new();
            for (k, v) in entries {
                obj.insert(k.clone(), vm_value_to_json(v));
            }
            JsonValue::Object(obj)
        }
        VmValue::Enum { tag, fields } => {
            let mut obj = serde_json::Map::new();
            obj.insert("$tag".to_string(), JsonValue::String(tag.clone()));
            let mut fields_obj = serde_json::Map::new();
            for (k, v) in fields {
                fields_obj.insert(k.clone(), vm_value_to_json(v));
            }
            obj.insert("$fields".to_string(), JsonValue::Object(fields_obj));
            JsonValue::Object(obj)
        }
    }
}
