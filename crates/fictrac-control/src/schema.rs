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
    variants.push(descriptor("record_start",object(json!({"directory":path(),"record_options":object(json!({"raw":{"type":"boolean"},"debug":{"type":"boolean"}}),&["raw","debug"])}),&["directory"])));
    for command in ["snapshot", "export"] {
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
        object(json!({"kind":{"const":"replace_exclusions"},"polygons":{"type":"array","maxItems":64,"items":points(3,256)}}),&["kind","polygons"]),
        object(json!({"kind":{"const":"forward"},"start":point(),"end":point()}),&["kind","start","end"]),
        object(json!({"kind":{"const":"square_cycle"}}),&["kind"]),
        object(json!({"kind":{"const":"resolution_reference"},"width":{"type":"integer","minimum":2,"maximum":32000000},"height":{"type":"integer","minimum":2,"maximum":32000000}}),&["kind"]),
        object(json!({"kind":{"const":"resolution_apply"},"width":{"type":"integer","minimum":2,"maximum":32000000},"height":{"type":"integer","minimum":2,"maximum":32000000}}),&["kind","width","height"]),
        object(json!({"kind":{"const":"square"},"plane":{"enum":["xy","yz","xz"]},"points":points(4,4)}),&["kind","plane","points"])
    ]});
    variants.push(descriptor("calibrate", calibration));
    variants.push(descriptor("view",object(json!({"settings":{"type":"boolean"},"diagnostics":{"type":"boolean"},"overlays":{"type":"boolean"},"full_image":{"type":"boolean"},"calibration_step":{"enum":["view","ball","exclude","lab"]},"calibration_action":{"enum":["inspect","edit","redraw","add","keep"]},"edit_marks":{"type":"boolean"},"magnifier":{"type":"boolean"}}),&[])));
    variants.push(descriptor("camera_status", none()));
    variants.push(descriptor("camera_format",object(json!({"width":{"type":"integer","minimum":16,"maximum":8192},"height":{"type":"integer","minimum":16,"maximum":8192},"fps":{"type":["number","null"],"minimum":0.1,"maximum":1000}}),&["width","height"])));
    variants.push(descriptor("camera_set",json!({"oneOf":[object(json!({"control":{"const":"exposure_mode"},"mode":{"enum":["automatic","locked"]}}),&["control","mode"]),object(json!({"control":{"enum":["exposure","gain"]},"value":{"type":"number"}}),&["control","value"])]})));

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
        "record_start" => &["directory", "record_options"],
        "snapshot" | "export" => &["directory"],
        "view" => &[
            "settings",
            "diagnostics",
            "overlays",
            "full_image",
            "calibration_step",
            "calibration_action",
            "edit_marks",
            "magnifier",
        ],
        "camera_status" => &[],
        "camera_format" => &["width", "height", "fps"],
        "camera_set" => &["control", "mode", "value"],
        "step" | "run" => &["frames"],
        "rows" => &["offset", "limit"],
        "calibrate" => &[
            "kind", "points", "start", "end", "plane", "width", "height", "polygons",
        ],
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
    for key in [
        "discard",
        "enabled",
        "settings",
        "diagnostics",
        "overlays",
        "full_image",
        "edit_marks",
        "magnifier",
    ] {
        if let Some(value) = object.get(key)
            && !value.is_boolean()
        {
            return Err(format!("args.{key} must be boolean"));
        }
    }
    if command == "view"
        && let Some(step) = object.get("calibration_step")
        && !step
            .as_str()
            .is_some_and(|s| matches!(s, "view" | "ball" | "exclude" | "lab"))
    {
        return Err("args.calibration_step must be view/ball/exclude/lab".into());
    }
    if command == "view"
        && let Some(action) = object.get("calibration_action")
        && !action
            .as_str()
            .is_some_and(|s| matches!(s, "inspect" | "edit" | "redraw" | "add" | "keep"))
    {
        return Err("args.calibration_action must be inspect/edit/redraw/add/keep".into());
    }
    if command == "calibrate" {
        for key in ["width", "height"] {
            if let Some(value) = object.get(key)
                && value
                    .as_u64()
                    .is_none_or(|n| !(2..=32_000_000).contains(&n))
            {
                return Err(format!("args.{key} must be an integer in 2..32000000"));
            }
        }
        if object.get("kind").and_then(Value::as_str) == Some("resolution_apply")
            && (!object.contains_key("width") || !object.contains_key("height"))
        {
            return Err("resolution_apply requires args.width and args.height".into());
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
        // Legacy field edits use string values; camera controls use driver-native numbers.
        if command == "camera_set" && key == "value" {
            continue;
        }
        if let Some(value) = object.get(key)
            && !value.is_string()
        {
            return Err(format!("args.{key} must be a string"));
        }
    }
    if let Some(options) = object.get("record_options") {
        serde_json::from_value::<crate::RecordOptions>(options.clone())
            .map_err(|e| format!("args.record_options: {e}"))?;
    }
    if command == "camera_set" {
        match object.get("control").and_then(Value::as_str) {
            Some("exposure_mode") => {
                if !matches!(
                    object.get("mode").and_then(Value::as_str),
                    Some("automatic" | "locked")
                ) || object.contains_key("value")
                {
                    return Err("exposure_mode requires mode automatic or locked".into());
                }
            }
            Some("exposure" | "gain") => {
                if !object.get("value").is_some_and(Value::is_number) || object.contains_key("mode")
                {
                    return Err("exposure/gain require a numeric value in the device-advertised unit and range".into());
                }
            }
            _ => return Err("camera control must be exposure_mode, exposure or gain".into()),
        }
    }
    if command == "camera_format" {
        for key in ["width", "height"] {
            if !object
                .get(key)
                .and_then(Value::as_u64)
                .is_some_and(|n| (16..=8192).contains(&n))
            {
                return Err(format!("args.{key} must be an integer 16..8192"));
            }
        }
        if object["width"]
            .as_u64()
            .zip(object["height"].as_u64())
            .is_some_and(|(width, height)| width * height > 8_000_000)
        {
            return Err("camera format exceeds 8 million pixels".into());
        }
        if let Some(fps) = object.get("fps")
            && !fps.is_null()
            && !fps.as_f64().is_some_and(|v| (0.1..=1000.).contains(&v))
        {
            return Err("args.fps must be null for automatic or a number 0.1..1000".into());
        }
    }
    Ok(())
}

/// The recorded-video controller does not advertise native-only actions.
pub fn headless_request_schema() -> Value {
    let mut schema = request_schema();
    if let Some(variants) = schema["oneOf"].as_array_mut() {
        variants.retain(|v| {
            !matches!(
                v["properties"]["command"]["const"].as_str(),
                Some("view" | "camera_status" | "camera_set" | "camera_format")
            )
        });
    }
    schema
}
