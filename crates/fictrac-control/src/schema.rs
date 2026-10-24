use serde_json::{Value, json};
fn string() -> Value {
    json!({"type":"string"})
}
fn path() -> Value {
    json!({"type":"string","minLength":1,"description":"Local filesystem path; output files/directories must be new"})
}
fn point() -> Value {
    json!({"type":"array","items":{"type":"number"},"minItems":2,"maxItems":2})
}
fn points(min: usize, max: usize) -> Value {
    json!({"type":"array","items":point(),"minItems":min,"maxItems":max})
}
fn object(properties: Value, required: &[&str]) -> Value {
    json!({"type":"object","properties":properties,"required":required,"additionalProperties":false})
}
fn descriptor(command: &str, args: Value) -> Value {
    let require_args = args
        .get("required")
        .and_then(Value::as_array)
        .is_some_and(|v| !v.is_empty())
        || args.get("anyOf").is_some()
        || args.get("oneOf").is_some();
    let mut properties = json!({"command":{"const":command},"id":{},"args":args,"session":{"type":"integer","minimum":1}});
    let global = matches!(command, "open" | "schema" | "discover" | "sessions");
    if global && let Some(p) = properties.as_object_mut() {
        p.remove("session");
    }
    let mut required = if global {
        vec!["command"]
    } else {
        vec!["command", "session"]
    };
    if require_args {
        required.push("args");
    }
    object(properties, &required)
}
/// JSON Schema 2020-12 for all concrete agent requests, also used by MCP discovery.
pub fn request_schema() -> Value {
    let none = || object(json!({}), &[]);
    let mut variants = vec![];
    for command in [
        "schema",
        "discover",
        "sessions",
        "new_instance",
        "status",
        "config",
        "undo",
        "redo",
        "start",
        "pause",
        "reset",
        "rewind",
        "record_stop",
    ] {
        variants.push(descriptor(command, none()));
    }
    variants.push(descriptor("close",object(json!({"discard":{"type":"boolean","default":false,"description":"Native GUI explicit discard of unsaved config edits; headless sessions are disposable in-memory state"}}),&[])));
    variants.push(descriptor("realtime",object(json!({"enabled":{"type":"boolean","description":"Pace source processing to video timestamps; headless applies during explicit step/run, native applies to the worker"}}),&["enabled"])));
    let mut open = object(json!({"config":path(),"text":string(),"input":path()}), &[]);
    open["anyOf"] = json!([{"required":["config"]},{"required":["text"]}]);
    variants.push(descriptor("open", open));
    let mut set = object(
        json!({"key":string(),"value":string(),"fields":{"type":"object","minProperties":1,"additionalProperties":{"type":"string"}}}),
        &[],
    );
    set["anyOf"] = json!([{"required":["key","value"]},{"required":["fields"]}]);
    variants.push(descriptor("set", set));
    variants.push(descriptor(
        "replace_config",
        object(json!({"text":string()}), &["text"]),
    ));
    variants.push(descriptor(
        "source",
        object(json!({"input":path()}), &["input"]),
    ));
    for command in ["save_config", "map_load", "map_save"] {
        variants.push(descriptor(
            command,
            object(json!({"path":path()}), &["path"]),
        ));
    }
    for command in ["record_start", "snapshot", "export"] {
        variants.push(descriptor(
            command,
            object(json!({"directory":path()}), &["directory"]),
        ));
    }
    for command in ["step", "run"] {
        variants.push(descriptor(
            command,
            object(
                json!({"frames":{"type":"integer","minimum":1,"maximum":10000}}),
                &[],
            ),
        ));
    }
    variants.push(descriptor("rows",object(json!({"offset":{"type":"integer","minimum":0},"limit":{"type":"integer","minimum":1,"maximum":10000}}),&[])));
    let calibration = json!({"oneOf":[
        object(json!({"kind":{"const":"rim"},"points":points(3,4096)}),&["kind","points"]),
        object(json!({"kind":{"const":"exclude"},"points":points(3,256)}),&["kind","points"]),
        object(json!({"kind":{"const":"remove_exclusion"}}),&["kind"]),
        object(json!({"kind":{"const":"forward"},"start":point(),"end":point()}),&["kind","start","end"]),
        object(json!({"kind":{"const":"square"},"plane":{"enum":["xy","yz","xz"]},"points":points(4,4)}),&["kind","plane","points"])
    ]});
    variants.push(descriptor("calibrate", calibration));
    json!({"$schema":"https://json-schema.org/draft/2020-12/schema","title":"FicTrac request","type":"object","oneOf":variants})
}
/// Reject misspelled arguments instead of silently accepting agent intent loss.
pub(crate) fn validate_arguments(command: &str, args: &Value) -> super::Result<()> {
    let allowed: &[&str] = match command {
        "schema" | "discover" | "sessions" | "new_instance" | "status" | "config" | "undo"
        | "redo" | "start" | "pause" | "reset" | "rewind" | "record_stop" => &[],
        "close" => &["discard"],
        "realtime" => &["enabled"],
        "open" => &["config", "text", "input"],
        "set" => &["key", "value", "fields"],
        "replace_config" => &["text"],
        "source" => &["input"],
        "save_config" | "map_load" | "map_save" => &["path"],
        "record_start" | "snapshot" | "export" => &["directory"],
        "step" | "run" => &["frames"],
        "rows" => &["offset", "limit"],
        "calibrate" => &["kind", "points", "start", "end", "plane"],
        _ => return Err(format!("unknown command {command}; use discover")),
    };
    if args.is_null() {
        return Ok(());
    }
    let object = args.as_object().ok_or("args must be an object")?;
    for key in object.keys() {
        if !allowed.contains(&key.as_str()) {
            return Err(format!("unknown args.{key} for {command}"));
        }
    }
    for key in ["discard", "enabled"] {
        if let Some(value) = object.get(key)
            && !value.is_boolean()
        {
            return Err(format!("args.{key} must be boolean"));
        }
    }
    for key in ["frames", "offset", "limit"] {
        if let Some(value) = object.get(key) {
            let n = value
                .as_u64()
                .ok_or_else(|| format!("args.{key} must be a nonnegative integer"))?;
            if key != "offset" && !(1..=10_000).contains(&n) {
                return Err(format!("args.{key} must be 1..10000"));
            }
        }
    }
    for key in [
        "config",
        "text",
        "input",
        "key",
        "value",
        "path",
        "directory",
        "kind",
        "plane",
    ] {
        if let Some(value) = object.get(key)
            && !value.is_string()
        {
            return Err(format!("args.{key} must be a string"));
        }
    }
    Ok(())
}
