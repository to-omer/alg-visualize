//! Exports the Rust-owned flow scene V9 structural contract for the web client.

use std::collections::BTreeMap;
use std::env;
use std::error::Error;
use std::ffi::OsStr;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use flow::{FlowCurrentSceneV9, flow_scene_schema_v9};
use serde_json::Value;
use ts_rs::{Config, TS};

const GENERATED_RELATIVE_PATH: &str = "apps/web/src/flow-scene-wire/generated";

fn main() -> Result<(), Box<dyn Error>> {
    let check = env::args().skip(1).any(|argument| argument == "--check");
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .ok_or("flow crate is not inside the workspace")?;
    let committed = workspace.join(GENERATED_RELATIVE_PATH);

    if check {
        let candidate = workspace
            .join("target")
            .join(format!("flow-scene-wire-codegen-{}", std::process::id()));
        if candidate.exists() {
            fs::remove_dir_all(&candidate)?;
        }
        generate(&candidate)?;
        let comparison = compare_directories(&candidate, &committed);
        fs::remove_dir_all(&candidate)?;
        comparison?;
        println!("flow scene V9 generated contract is current");
    } else {
        if committed.exists() {
            fs::remove_dir_all(&committed)?;
        }
        generate(&committed)?;
        println!("generated {}", committed.display());
    }
    Ok(())
}

fn generate(destination: &Path) -> Result<(), Box<dyn Error>> {
    fs::create_dir_all(destination)?;
    let config = Config::new()
        .with_out_dir(destination)
        .with_import_extension(Some("js"));
    FlowCurrentSceneV9::export_all(&config)?;
    normalize_ts_rs_bindings(destination)?;
    write_type_barrel(destination)?;

    let mut schema = serde_json::to_value(flow_scene_schema_v9())?;
    normalize_serialized_schema(&mut schema);
    let mut schema_source =
        String::from("// Generated from Rust FlowCurrentSceneV9 by export_flow_scene_contract.\n");
    schema_source
        .push_str("// Do not edit manually.\n\nexport const FLOW_SCENE_V9_SCHEMA: unknown = ");
    schema_source.push_str(&serde_json::to_string_pretty(&schema)?);
    schema_source.push_str(" as const;\n");
    fs::write(destination.join("schema.ts"), schema_source)?;

    let overlays = overlay_definitions(&schema)?;
    let overlay_directory = destination.join("overlays");
    fs::create_dir_all(&overlay_directory)?;
    for (field, definition) in &overlays {
        fs::write(
            overlay_directory.join(format!("{field}.ts")),
            overlay_module(field, definition),
        )?;
    }
    fs::write(
        overlay_directory.join("index.ts"),
        overlay_registry(&overlays),
    )?;
    Ok(())
}

fn normalize_ts_rs_bindings(destination: &Path) -> Result<(), Box<dyn Error>> {
    for path in files_below(destination)?.into_values() {
        let source = fs::read_to_string(&path)?;
        let normalized = source.split_inclusive('\n').fold(
            String::with_capacity(source.len()),
            |mut output, line| {
                let (body, newline) = line
                    .strip_suffix('\n')
                    .map_or((line, ""), |body| (body, "\n"));
                output.push_str(body.trim_end_matches([' ', '\t']));
                output.push_str(newline);
                output
            },
        );
        if normalized != source {
            fs::write(path, normalized)?;
        }
    }
    Ok(())
}

fn write_type_barrel(destination: &Path) -> Result<(), Box<dyn Error>> {
    let mut type_names = fs::read_dir(destination)?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && path.extension() == Some(OsStr::new("ts")))
        .filter_map(|path| path.file_stem().and_then(OsStr::to_str).map(str::to_owned))
        .collect::<Vec<_>>();
    type_names.sort();

    let mut source = String::from(
        "// Generated from Rust FlowCurrentSceneV9 by export_flow_scene_contract.\n\
         // Do not edit manually.\n\n",
    );
    for type_name in type_names {
        writeln!(
            &mut source,
            "export type {{ {type_name} }} from \"./{type_name}.js\";"
        )?;
    }
    fs::write(destination.join("types.ts"), source)?;
    Ok(())
}

fn overlay_definitions(schema: &Value) -> Result<BTreeMap<String, String>, Box<dyn Error>> {
    let properties = schema
        .get("properties")
        .and_then(Value::as_object)
        .ok_or("FlowCurrentSceneV9 schema has no root properties")?;
    let mut overlays = BTreeMap::new();
    for (field, property) in properties {
        if !field.ends_with("_overlay") {
            continue;
        }
        let reference = find_reference(property)
            .ok_or_else(|| format!("overlay {field} has no schema reference"))?;
        let definition = reference
            .strip_prefix("#/$defs/")
            .ok_or_else(|| format!("overlay {field} has unsupported reference {reference}"))?;
        overlays.insert(field.clone(), definition.to_owned());
    }
    Ok(overlays)
}

fn find_reference(value: &Value) -> Option<&str> {
    if let Some(reference) = value.get("$ref").and_then(Value::as_str) {
        return Some(reference);
    }
    value
        .get("anyOf")
        .or_else(|| value.get("oneOf"))
        .and_then(Value::as_array)
        .and_then(|items| items.iter().find_map(find_reference))
}

fn normalize_serialized_schema(value: &mut Value) {
    match value {
        Value::Array(items) => {
            for item in items {
                normalize_serialized_schema(item);
            }
        }
        Value::Object(object) => {
            for annotation in [
                "$schema",
                "default",
                "description",
                "examples",
                "format",
                "title",
            ] {
                object.remove(annotation);
            }
            let required = object
                .get("required")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            if let Some(Value::Object(properties)) = object.get_mut("properties") {
                for (field, property) in properties {
                    normalize_serialized_schema(property);
                    if !required.contains(field) {
                        remove_null_from_optional_property(property);
                    }
                }
            }
            for (key, child) in object {
                if key != "properties" {
                    normalize_serialized_schema(child);
                }
            }
        }
        _ => {}
    }
}

fn remove_null_from_optional_property(value: &mut Value) {
    let Some(object) = value.as_object_mut() else {
        return;
    };
    for union_key in ["anyOf", "oneOf"] {
        let Some(Value::Array(items)) = object.get_mut(union_key) else {
            continue;
        };
        items.retain(|item| item.get("type").and_then(Value::as_str) != Some("null"));
        if items.len() == 1 {
            let only = items.pop().expect("length checked");
            *value = only;
        }
        return;
    }
}

fn overlay_module(field: &str, definition: &str) -> String {
    format!(
        "// Generated from Rust FlowCurrentSceneV9 by export_flow_scene_contract.\n\
         // Do not edit manually.\n\n\
         import type {{ {definition} }} from \"../{definition}.js\";\n\
         import {{ assertFlowSceneDefinition }} from \"../../schema-validator\";\n\n\
         export const FIELD = \"{field}\" as const;\n\
         export const DEFINITION = \"{definition}\" as const;\n\n\
         export function decodeStructure(value: unknown): {definition} {{\n\
         \tassertFlowSceneDefinition(value, DEFINITION, FIELD);\n\
         \treturn value as {definition};\n\
         }}\n",
    )
}

fn overlay_registry(overlays: &BTreeMap<String, String>) -> String {
    let mut source = String::from(
        "// Generated from Rust FlowCurrentSceneV9 by export_flow_scene_contract.\n\
         // Do not edit manually.\n\n\
         import type { FlowCurrentSceneV9 } from \"../FlowCurrentSceneV9.js\";\n\n\
         export type FlowSceneV9OverlayField = Extract<\n\
         \tkeyof FlowCurrentSceneV9,\n\
         \t`${string}_overlay`\n\
         >;\n\
         export type FlowSceneV9OverlayDecoder = (value: unknown) => unknown;\n\n",
    );
    for (index, field) in overlays.keys().enumerate() {
        writeln!(
            &mut source,
            "import {{ decodeStructure as decode{index} }} from \"./{field}.js\";"
        )
        .expect("writing to a String cannot fail");
    }
    source.push_str(
        "\nexport const FLOW_SCENE_V9_OVERLAY_DECODERS: ReadonlyArray<\n\
         \treadonly [field: FlowSceneV9OverlayField, decode: FlowSceneV9OverlayDecoder]\n\
         > = [\n",
    );
    for (index, field) in overlays.keys().enumerate() {
        writeln!(&mut source, "\t[\"{field}\", decode{index}],")
            .expect("writing to a String cannot fail");
    }
    source.push_str("] as const;\n");
    source
}

fn compare_directories(actual: &Path, expected: &Path) -> Result<(), Box<dyn Error>> {
    let actual_files = files_below(actual)?;
    let expected_files = files_below(expected)?;
    if actual_files.keys().collect::<Vec<_>>() != expected_files.keys().collect::<Vec<_>>() {
        return Err(format!(
            "generated flow scene contract file set differs; run `cargo run -p flow --bin export_flow_scene_contract`\nactual: {:?}\nexpected: {:?}",
            actual_files.keys().collect::<Vec<_>>(),
            expected_files.keys().collect::<Vec<_>>()
        )
        .into());
    }
    for (relative, actual_path) in actual_files {
        let expected_path = expected_files
            .get(&relative)
            .ok_or("generated contract comparison lost a path")?;
        if fs::read(&actual_path)? != fs::read(expected_path)? {
            return Err(format!(
                "generated flow scene contract differs at {}; run `cargo run -p flow --bin export_flow_scene_contract`",
                relative.display()
            )
            .into());
        }
    }
    Ok(())
}

fn files_below(root: &Path) -> Result<BTreeMap<PathBuf, PathBuf>, Box<dyn Error>> {
    if !root.is_dir() {
        return Err(format!(
            "generated contract directory is missing: {}",
            root.display()
        )
        .into());
    }
    let mut pending = vec![root.to_owned()];
    let mut files = BTreeMap::new();
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(&directory)? {
            let path = entry?.path();
            if path.is_dir() {
                pending.push(path);
            } else if path.extension() == Some(OsStr::new("ts")) {
                files.insert(path.strip_prefix(root)?.to_owned(), path);
            }
        }
    }
    Ok(files)
}
