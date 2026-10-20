//! MCP server for the Cinnabar CKB contract framework.
//!
//! The binary speaks JSON-RPC 2.0 on stdin/stdout. Library functions behind
//! each tool are pure enough to unit test without a language-client: error
//! codes, operation catalogs, doc lookup, and one in-process FakeRpc recipe.
//!
//! # Framing
//!
//! [`frame`] / [`read_frame`] use LSP `Content-Length` headers. [`read_frame`]
//! also accepts one JSON object per line so a pipe can skip the header.

#![warn(missing_docs)]

use std::{
    fs,
    io::{self, BufRead},
    path::{Path, PathBuf},
    process::Command,
};

use serde_json::{json, Value};

mod simulate;

pub use simulate::{always_success_transfer, simulate, SimulationReport};

const PROTOCOL: &str = "2024-11-05";

/// Serialize `value` as one LSP frame.
pub fn frame(value: &Value) -> Vec<u8> {
    let body = serde_json::to_vec(value).unwrap_or_else(|_| b"{}".to_vec());
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    let mut out = header.into_bytes();
    out.extend(body);
    out
}

/// Read one JSON-RPC message. `Ok(None)` is EOF.
pub fn read_frame<R: BufRead>(reader: &mut R) -> io::Result<Option<Value>> {
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Ok(None);
    }
    if line.trim_start().starts_with('{') {
        let value = serde_json::from_str(line.trim())
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
        return Ok(Some(value));
    }
    let mut content_length = header_length(&line);
    loop {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some(len) = header_length(&line) {
            content_length = Some(len);
        }
    }
    let len = content_length
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing Content-Length"))?;
    let mut buf = vec![0u8; len];
    reader.read_exact(&mut buf)?;
    let value = serde_json::from_slice(&buf)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err.to_string()))?;
    Ok(Some(value))
}

fn header_length(line: &str) -> Option<usize> {
    let rest = line
        .split_once(':')
        .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))?
        .1;
    rest.trim().parse().ok()
}

/// Handle one JSON-RPC message. Notifications return `None`.
pub fn handle_message(message: &Value) -> Option<Value> {
    let id = message.get("id").cloned();
    if id.is_none() {
        return None;
    }
    let method = message.get("method").and_then(Value::as_str).unwrap_or("");
    let params = message.get("params").cloned().unwrap_or(json!({}));
    let result = match method {
        "initialize" => Ok(initialize()),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tool_defs() })),
        "tools/call" => call_tool(&params),
        "resources/list" => Ok(json!({ "resources": resource_defs() })),
        "resources/read" => read_resource(&params),
        other => Err(format!("unknown method {other}")),
    };
    Some(match result {
        Ok(value) => json!({ "jsonrpc": "2.0", "id": id, "result": value }),
        Err(message) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32000, "message": message }
        }),
    })
}

fn initialize() -> Value {
    json!({
        "protocolVersion": PROTOCOL,
        "capabilities": { "tools": { "listChanged": false }, "resources": {} },
        "serverInfo": { "name": "ckb-cinnabar-mcp", "version": env!("CARGO_PKG_VERSION") },
        "instructions": "Follow AGENTS.md. Scaffold from templates/contract. make build && make test must both exit 0."
    })
}

fn tool_defs() -> Vec<Value> {
    vec![
        tool(
            "scaffold_project",
            "Generate a contract project from templates/contract.",
            json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string" },
                    "kind": { "type": "string", "enum": ["lock", "type", "contract"] },
                    "execute": { "type": "boolean", "description": "Run cargo generate. Default true." }
                },
                "required": ["name"]
            }),
        ),
        tool(
            "build",
            "Run make build in a generated project.",
            json!({
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            }),
        ),
        tool(
            "test",
            "Run make test in a generated project.",
            json!({
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            }),
        ),
        tool(
            "simulate",
            "FakeRpc simulation. recipe=always_success_transfer.",
            json!({
                "type": "object",
                "properties": {
                    "recipe": { "type": "string" },
                    "capacity_in": { "type": "integer" },
                    "capacity_out": { "type": "integer" }
                }
            }),
        ),
        tool(
            "explain_error",
            "Explain a CKB script exit code.",
            json!({
                "type": "object",
                "properties": {
                    "exit_code": { "type": "integer" },
                    "contract": { "type": "string" }
                },
                "required": ["exit_code"]
            }),
        ),
        tool(
            "list_operations",
            "List Calculate operations agents compose.",
            json!({ "type": "object" }),
        ),
        tool(
            "list_recipes",
            "List named instruction recipes.",
            json!({ "type": "object" }),
        ),
        tool(
            "get_docs",
            "Read one framework doc topic.",
            json!({
                "type": "object",
                "properties": { "topic": { "type": "string" } },
                "required": ["topic"]
            }),
        ),
        tool(
            "deploy_dry_run",
            "ckb-cinnabar --json --dry-run deploy.",
            json!({
                "type": "object",
                "properties": {
                    "contract": { "type": "string" },
                    "network": { "type": "string" },
                    "execute": { "type": "boolean" }
                },
                "required": ["contract"]
            }),
        ),
        tool(
            "list_deployments",
            "Read deployment/<network>/*.json.",
            json!({
                "type": "object",
                "properties": { "path": { "type": "string" } },
                "required": ["path"]
            }),
        ),
        tool(
            "query_cells",
            "Describe or run a cell query. execute=false skips RPC.",
            json!({
                "type": "object",
                "properties": {
                    "address": { "type": "string" },
                    "script": { "type": "string" },
                    "network": { "type": "string" },
                    "execute": { "type": "boolean" }
                }
            }),
        ),
    ]
}

fn tool(name: &str, description: &str, schema: Value) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": schema
    })
}

fn call_tool(params: &Value) -> Result<Value, String> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or("tools/call needs name")?;
    let args = params.get("arguments").cloned().unwrap_or(json!({}));
    let value = match name {
        "scaffold_project" => scaffold_project(&args)?,
        "build" => run_make(&args, "build")?,
        "test" => run_make(&args, "test")?,
        "simulate" => block_on_simulate(&args)?,
        "explain_error" => explain_error(&args)?,
        "list_operations" => json!({ "operations": operations() }),
        "list_recipes" => json!({ "recipes": recipes() }),
        "get_docs" => get_docs(&args)?,
        "deploy_dry_run" => deploy_dry_run(&args)?,
        "list_deployments" => list_deployments(&args)?,
        "query_cells" => query_cells(&args)?,
        other => return Err(format!("unknown tool {other}")),
    };
    Ok(json!({
        "content": [{ "type": "text", "text": serde_json::to_string_pretty(&value).unwrap_or_default() }],
        "structuredContent": value,
        "isError": false
    }))
}

fn block_on_simulate(args: &Value) -> Result<Value, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| err.to_string())?;
    runtime.block_on(simulate(args))
}

/// Framework checkout that contains `AGENTS.md` and `templates/contract`.
pub fn find_repo_root(start: &Path) -> Option<PathBuf> {
    if let Some(env) = std::env::var_os("CINNABAR_ROOT") {
        let path = PathBuf::from(env);
        if path.join("AGENTS.md").is_file() {
            return Some(path);
        }
    }
    let mut cursor = Some(start);
    while let Some(dir) = cursor {
        if dir.join("AGENTS.md").is_file() && dir.join("templates/contract").is_dir() {
            return Some(dir.to_path_buf());
        }
        cursor = dir.parent();
    }
    None
}

fn repo_root() -> Result<PathBuf, String> {
    find_repo_root(&std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .ok_or_else(|| "set CINNABAR_ROOT to a cinnabar checkout".into())
}

/// `cargo generate` plan for `templates/contract`.
pub fn scaffold_plan(root: &Path, name: &str, kind: &str) -> Result<Value, String> {
    if !name_ok(name) {
        return Err("name must match [a-z][a-z0-9_-]{0,63}".into());
    }
    let place = match kind {
        "lock" => "ScriptPlace::Lock",
        "type" => "ScriptPlace::Type",
        "contract" => "choose Lock or Type in the verification tree",
        other => {
            return Err(format!(
                "kind must be lock, type, or contract (got {other})"
            ))
        }
    };
    let template = root.join("templates/contract");
    let command = format!(
        "cargo generate --path {} --name {name} --define use_local_cinnabar=true --define cinnabar_path={}",
        template.display(),
        root.display()
    );
    Ok(json!({
        "name": name,
        "kind": kind,
        "script_place": place,
        "command": command,
        "note": "Generated Root uses ScriptPlace::Lock. Change it when kind is type. Then make build && make test."
    }))
}

fn name_ok(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first.is_ascii_lowercase() => {}
        _ => return false,
    }
    name.len() <= 64
        && name
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '_' || ch == '-')
}

fn scaffold_project(args: &Value) -> Result<Value, String> {
    let name = args
        .get("name")
        .and_then(Value::as_str)
        .ok_or("name is required")?;
    let kind = args.get("kind").and_then(Value::as_str).unwrap_or("lock");
    let root = repo_root()?;
    let mut plan = scaffold_plan(&root, name, kind)?;
    let execute = args.get("execute").and_then(Value::as_bool).unwrap_or(true);
    if !execute {
        plan["executed"] = json!(false);
        return Ok(plan);
    }
    let status = Command::new("cargo")
        .args([
            "generate",
            "--path",
            &root.join("templates/contract").to_string_lossy(),
            "--name",
            name,
            "--define",
            "use_local_cinnabar=true",
            "--define",
            &format!("cinnabar_path={}", root.display()),
        ])
        .output();
    match status {
        Ok(output) => {
            plan["executed"] = json!(true);
            plan["ok"] = json!(output.status.success());
            plan["stdout"] = json!(String::from_utf8_lossy(&output.stdout));
            plan["stderr"] = json!(String::from_utf8_lossy(&output.stderr));
            plan["exit_code"] = json!(output.status.code());
            Ok(plan)
        }
        Err(err) => {
            plan["executed"] = json!(false);
            plan["ok"] = json!(false);
            plan["kind"] = json!("missing_tool");
            plan["message"] = json!(err.to_string());
            Ok(plan)
        }
    }
}

/// Run `make <target>` and capture both streams.
pub fn run_make_at(path: &Path, target: &str) -> Value {
    match Command::new("make").arg(target).current_dir(path).output() {
        Ok(output) => json!({
            "ok": output.status.success(),
            "exit_code": output.status.code().unwrap_or(1),
            "stdout": String::from_utf8_lossy(&output.stdout),
            "stderr": String::from_utf8_lossy(&output.stderr),
        }),
        Err(err) => json!({
            "ok": false,
            "exit_code": 1,
            "kind": "io",
            "message": err.to_string(),
            "stdout": "",
            "stderr": "",
        }),
    }
}

fn run_make(args: &Value, target: &str) -> Result<Value, String> {
    let path = args
        .get("path")
        .and_then(Value::as_str)
        .ok_or("path is required")?;
    Ok(run_make_at(Path::new(path), target))
}

/// Lookup for script exit codes. Codes ≥ 20 are contract-defined.
pub fn explain_exit(exit_code: i64, contract: Option<&str>) -> Value {
    let (name, meaning) = match exit_code {
        0 => ("Success", "The script accepted the transaction."),
        1 => ("IndexOutOfBound", "A syscall index was out of range."),
        2 => (
            "ItemMissing",
            "The requested cell, witness, or header is absent.",
        ),
        3 => ("LengthNotEnough", "A syscall buffer was too small."),
        4 => ("Encoding", "Molecule or syscall bytes did not decode."),
        5 => (
            "UnknownSystemError",
            "ckb-std returned some other SysError.",
        ),
        10 => (
            "NotFoundRootVerifier",
            "No node is registered under TREE_ROOT.",
        ),
        11 => (
            "NotFoundBranchVerifier",
            "The next hop name is not registered, or the walk cycled.",
        ),
        12 => ("SSRIMethodsNotFound", "argv[0] is not in the SSRI table."),
        13 => (
            "SSRIMethodsArgsInvalid",
            "An SSRI argument failed to decode.",
        ),
        14 => (
            "SSRIMethodsNotImplemented",
            "The method id is known but not implemented.",
        ),
        15 => (
            "SSRIMethodRequireHigherLevel",
            "The host execution level is too low.",
        ),
        16 => (
            "InvalidVmVersion",
            "argv is set but vm_version is not u64::MAX.",
        ),
        17 => (
            "SSRIAssembleFailed",
            "A kernel Instruction failed while serving the method.",
        ),
        18 => ("SSRISourceUnavailable", "A guest Source lookup failed."),
        code if code >= 20 => (
            "Custom",
            "Declared with define_errors! in the contract. This server does not guess it.",
        ),
        _ => ("Unknown", "Not a Cinnabar framework code."),
    };
    json!({
        "exit_code": exit_code,
        "name": name,
        "meaning": meaning,
        "contract": contract,
        "docs": "docs/error-codes.md",
    })
}

fn explain_error(args: &Value) -> Result<Value, String> {
    let code = args
        .get("exit_code")
        .and_then(Value::as_i64)
        .ok_or("exit_code is required")?;
    let contract = args.get("contract").and_then(Value::as_str);
    Ok(explain_exit(code, contract))
}

/// Operation names an agent composes into an `Instruction`.
pub fn operations() -> Vec<Value> {
    const ROWS: &[(&str, &str)] = &[
        ("AddInputCell", "basic"),
        ("AddInputCellByOutPoint", "basic"),
        ("AddInputCellByType", "basic"),
        ("AddInputCellByAddress", "basic (shell)"),
        ("AddOutputCell", "basic"),
        ("AddOutputCellByInputIndex", "basic"),
        ("AddCellDep", "basic"),
        ("AddCellDepByType", "basic"),
        ("AddCellDepByTypeId", "basic"),
        ("AddHeaderDep", "basic"),
        ("AddWitnessArgs", "basic"),
        ("AddInjectedInput", "basic"),
        ("AddInjectedCellDep", "basic"),
        ("AddSecp256k1SighashCellDep", "basic (shell)"),
        ("AddSecp256k1SighashSignatures", "basic (shell)"),
        ("BalanceTransaction", "basic (shell)"),
        ("AddDaoDepositOutputCell", "dao"),
        ("AddDaoWithdrawPhaseOneCells", "dao"),
        ("AddDaoWithdrawPhaseTwoCells", "dao (shell)"),
        ("AddDaoCelldep", "dao"),
        ("AddXudtOutputCell", "udt"),
        ("AddXudtTransferCells", "udt"),
        ("AddXudtCelldep", "udt"),
        ("AddSporeOutputCell", "spore"),
        ("AddClusterOutputCell", "spore"),
        ("AddFakeInputCell", "simulation"),
        ("AddFakeContractCelldepByName", "simulation"),
        ("AddFakeAlwaysSuccessCelldep", "simulation"),
    ];
    ROWS.iter()
        .map(|(name, module)| json!({ "name": name, "module": module }))
        .collect()
}

/// Named recipes in `instruction::predefined`.
pub fn recipes() -> Vec<Value> {
    const ROWS: &[(&str, &str)] = &[
        ("secp256k1_sighash_transfer", "transfer"),
        ("balance_and_sign", ""),
        ("balance_and_sign_with_ckb_cli", ""),
        ("dao_deposit", "deposit"),
        ("dao_withdraw_phase_one", "withdraw"),
        ("dao_withdraw_phase_two", "withdraw"),
        ("mint_xudt", "mint"),
        ("transfer_xudt", "transfer"),
        ("mint_spores", "mint (feature spore)"),
        ("transfer_spores", "transfer (feature spore)"),
        ("burn_spores", "burn (feature spore)"),
    ];
    ROWS.iter()
        .map(|(name, intent)| json!({ "name": name, "intent": intent }))
        .collect()
}

const TOPICS: &[(&str, &str)] = &[
    ("agents", "AGENTS.md"),
    ("skill", "skills/cinnabar-agent/SKILL.md"),
    ("readme", "README.md"),
    ("errors", "docs/error-codes.md"),
    ("llms", "llms.txt"),
    ("contributing", "CONTRIBUTING.md"),
    ("template", "templates/contract/README.md"),
    ("scenarios", "examples/scenarios/README.md"),
];

/// Read one doc topic from `root`.
pub fn read_topic(root: &Path, topic: &str) -> Result<Value, String> {
    let Some((_, rel)) = TOPICS.iter().find(|(name, _)| *name == topic) else {
        let names: Vec<&str> = TOPICS.iter().map(|(name, _)| *name).collect();
        return Err(format!("unknown topic {topic}; try {}", names.join(", ")));
    };
    let path = root.join(rel);
    let text = fs::read_to_string(&path).map_err(|err| format!("{}: {err}", path.display()))?;
    Ok(json!({ "topic": topic, "path": rel, "text": text }))
}

fn get_docs(args: &Value) -> Result<Value, String> {
    let topic = args
        .get("topic")
        .and_then(Value::as_str)
        .ok_or("topic is required")?;
    read_topic(&repo_root()?, topic)
}

/// Argv for a headless deploy that does not send.
pub fn deploy_argv(contract: &str, network: &str) -> Vec<String> {
    vec![
        "--json".into(),
        "--dry-run".into(),
        "--network".into(),
        network.into(),
        "deploy".into(),
        "--contract-name".into(),
        contract.into(),
        "--tag".into(),
        "v0.1.0".into(),
    ]
}

fn deploy_dry_run(args: &Value) -> Result<Value, String> {
    let contract = args
        .get("contract")
        .and_then(Value::as_str)
        .ok_or("contract is required")?;
    let network = args
        .get("network")
        .and_then(Value::as_str)
        .unwrap_or("testnet");
    let argv = deploy_argv(contract, network);
    let execute = args.get("execute").and_then(Value::as_bool).unwrap_or(true);
    if !execute {
        return Ok(json!({ "executed": false, "argv": argv }));
    }
    let bin = std::env::var("CINNABAR_BIN").unwrap_or_else(|_| "ckb-cinnabar".into());
    match Command::new(&bin).args(&argv).output() {
        Ok(output) => Ok(json!({
            "executed": true,
            "ok": output.status.success(),
            "exit_code": output.status.code(),
            "stdout": String::from_utf8_lossy(&output.stdout),
            "stderr": String::from_utf8_lossy(&output.stderr),
            "argv": argv,
        })),
        Err(err) => Ok(json!({
            "executed": false,
            "ok": false,
            "kind": "cli_not_found",
            "message": err.to_string(),
            "argv": argv,
        })),
    }
}

/// Parse every `*.json` file in `dir` (a `deployment/` folder or one network).
pub fn read_deployments(dir: &Path) -> Result<Vec<Value>, String> {
    if !dir.is_dir() {
        return Err(format!("{} is not a directory", dir.display()));
    }
    let mut files = Vec::new();
    let entries = fs::read_dir(dir).map_err(|err| err.to_string())?;
    for entry in entries {
        let entry = entry.map_err(|err| err.to_string())?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            if path.is_dir() {
                if let Ok(nested) = read_deployments(&path) {
                    files.extend(nested);
                }
            }
            continue;
        }
        let text = fs::read_to_string(&path).map_err(|err| err.to_string())?;
        let parsed = serde_json::from_str::<Value>(&text).unwrap_or(json!({ "raw": text }));
        files.push(json!({ "path": path.display().to_string(), "record": parsed }));
    }
    Ok(files)
}

fn list_deployments(args: &Value) -> Result<Value, String> {
    let path = args
        .get("path")
        .and_then(Value::as_str)
        .ok_or("path is required")?;
    let records = read_deployments(Path::new(path))?;
    Ok(json!({ "records": records }))
}

/// Describe a cell query without calling RPC when `execute` is false.
pub fn query_plan(
    address: Option<&str>,
    script: Option<&str>,
    network: &str,
) -> Result<Value, String> {
    if address.map(str::trim).filter(|s| !s.is_empty()).is_none()
        && script.map(str::trim).filter(|s| !s.is_empty()).is_none()
    {
        return Err("address or script is required".into());
    }
    if network.trim().is_empty() {
        return Err("network is required".into());
    }
    Ok(json!({
        "address": address,
        "script": script,
        "network": network,
        "order": "asc",
        "note": "Live search uses Indexer::get_cells. Pass execute=true with a URL or mainnet/testnet to call RpcClient."
    }))
}

fn query_cells(args: &Value) -> Result<Value, String> {
    let address = args.get("address").and_then(Value::as_str);
    let script = args.get("script").and_then(Value::as_str);
    let network = args
        .get("network")
        .and_then(Value::as_str)
        .unwrap_or("testnet");
    let mut plan = query_plan(address, script, network)?;
    let execute = args
        .get("execute")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if !execute {
        plan["executed"] = json!(false);
        return Ok(plan);
    }
    plan["executed"] = json!(false);
    plan["kind"] = json!("not_run");
    plan["message"] = json!(
        "Live indexer calls stay on RpcClient in the calculator. This scaffold returns the query so an agent can run it with a configured RPC URL."
    );
    Ok(plan)
}

fn resource_defs() -> Vec<Value> {
    [
        "AGENTS.md",
        "skills/cinnabar-agent/SKILL.md",
        "docs/error-codes.md",
        "llms.txt",
        "README.md",
        "examples/xudt.rs",
        "examples/multisig.rs",
        "examples/timelock.rs",
        "examples/scenarios/README.md",
    ]
    .into_iter()
    .map(|path| {
        json!({
            "uri": format!("cinnabar://{path}"),
            "name": path,
            "mimeType": if path.ends_with(".rs") { "text/rust" } else { "text/markdown" }
        })
    })
    .collect()
}

fn read_resource(params: &Value) -> Result<Value, String> {
    let uri = params
        .get("uri")
        .and_then(Value::as_str)
        .ok_or("resources/read needs uri")?;
    let rel = uri
        .strip_prefix("cinnabar://")
        .ok_or_else(|| format!("unsupported uri {uri}"))?;
    if rel.contains("..") {
        return Err("uri must stay inside the repo".into());
    }
    let path = repo_root()?.join(rel);
    let text = fs::read_to_string(&path).map_err(|err| format!("{}: {err}", path.display()))?;
    Ok(json!({
        "contents": [{
            "uri": uri,
            "mimeType": "text/plain",
            "text": text
        }]
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explain_known_and_custom_codes() {
        assert_eq!(explain_exit(11, None)["name"], "NotFoundBranchVerifier");
        assert_eq!(explain_exit(20, Some("timelock"))["contract"], "timelock");
        assert_eq!(explain_exit(20, None)["name"], "Custom");
        assert_eq!(explain_exit(99, None)["name"], "Custom");
        assert_eq!(explain_exit(7, None)["name"], "Unknown");
    }

    #[test]
    fn catalogs_include_xudt_and_transfer() {
        let ops = operations();
        assert!(ops.iter().any(|row| row["name"] == "AddOutputCell"));
        assert!(ops.iter().any(|row| row["name"] == "AddXudtOutputCell"));
        let recipes = recipes();
        assert!(recipes.iter().any(|row| row["name"] == "mint_xudt"));
        assert!(recipes.iter().any(|row| row["name"] == "transfer_xudt"));
    }

    #[test]
    fn scaffold_plan_rejects_bad_names() {
        let root = Path::new("/tmp/cinnabar");
        assert!(scaffold_plan(root, "MyLock", "lock").is_err());
        let plan = scaffold_plan(root, "my-lock", "type").unwrap();
        assert_eq!(plan["script_place"], "ScriptPlace::Type");
        assert!(plan["command"]
            .as_str()
            .unwrap()
            .contains("templates/contract"));
    }

    #[test]
    fn deploy_argv_is_dry_run_json() {
        let argv = deploy_argv("my_lock", "testnet");
        assert!(argv.windows(2).any(|pair| pair == ["--json", "--dry-run"]));
        assert!(argv.iter().any(|arg| arg == "my_lock"));
    }

    #[test]
    fn query_plan_needs_a_target() {
        assert!(query_plan(None, None, "testnet").is_err());
        let plan = query_plan(Some("ckt1qq"), None, "testnet").unwrap();
        assert_eq!(plan["order"], "asc");
    }

    #[test]
    fn tools_list_covers_the_scaffold() {
        let response = handle_message(&json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/list"
        }))
        .unwrap();
        let names: Vec<&str> = response["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|tool| tool["name"].as_str().unwrap())
            .collect();
        for required in [
            "scaffold_project",
            "build",
            "test",
            "simulate",
            "explain_error",
            "list_operations",
            "get_docs",
            "deploy_dry_run",
        ] {
            assert!(names.contains(&required), "missing {required}");
        }
    }

    #[test]
    fn explain_tool_round_trip() {
        let response = handle_message(&json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": { "name": "explain_error", "arguments": { "exit_code": 10 } }
        }))
        .unwrap();
        let structured = &response["result"]["structuredContent"];
        assert_eq!(structured["name"], "NotFoundRootVerifier");
    }

    #[test]
    fn frame_round_trip() {
        let bytes = frame(&json!({"jsonrpc":"2.0","id":1,"method":"ping"}));
        let value = read_frame(&mut io::Cursor::new(bytes)).unwrap().unwrap();
        assert_eq!(value["method"], "ping");
    }

    #[test]
    fn make_captures_a_failing_makefile() {
        let dir = std::env::temp_dir().join(format!("cinnabar-mcp-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("Makefile"),
            "build:\n\t@echo nope >&2\n\t@exit 3\n",
        )
        .unwrap();
        let report = run_make_at(&dir, "build");
        assert_eq!(report["ok"], false);
        // GNU make exits 2 when a recipe fails. The recipe status is in stderr
        // (`Error 3`), not in make's process status.
        assert_eq!(report["exit_code"], 2);
        assert!(
            report["stderr"].as_str().unwrap_or("").contains("Error 3"),
            "{}",
            report["stderr"]
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn deployments_read_nested_json() {
        let dir = std::env::temp_dir().join(format!("cinnabar-dep-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("testnet")).unwrap();
        fs::write(
            dir.join("testnet/my_lock.json"),
            r#"{"contract_name":"my_lock"}"#,
        )
        .unwrap();
        let records = read_deployments(&dir).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0]["record"]["contract_name"], "my_lock");
        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn always_success_simulation_exits_zero() {
        let report = always_success_transfer(200_000_000_000, 100_000_000_000).await;
        assert_eq!(report.exit_code, 0, "{}", report.message);
        assert!(report.cycles > 0);
    }
}
