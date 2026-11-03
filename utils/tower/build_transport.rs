//! Shared postprocessing of pinned tonic-prost-build output. Fail closed if its
//! service shape changes; protobuf definitions and all field tags remain intact.
#[path = "build_wire_preflight.rs"]
mod build_wire_preflight;
use std::{fs, path::Path};

pub fn budget_generated_transport(path: &Path, request: &str, response: &str) {
    let source = fs::read_to_string(path).expect("read generated transport");
    let source = source.replace("tonic_prost::ProstCodec::default()", "kaspa_utils_tower::transport::BudgetedProstCodec::default()");
    let outbound = |name: &str| format!("kaspa_utils_tower::transport::Outbound<super::{name}>");
    let replacements = [
        (
            format!("IntoStreamingRequest<Message = super::{request}>"),
            format!("IntoStreamingRequest<Message = {}>", outbound(request)),
        ),
        (
            format!("Item = std::result::Result<super::{response}, tonic::Status>"),
            format!("Item = std::result::Result<{}, tonic::Status>", outbound(response)),
        ),
        (format!("type Response = super::{response};"), format!("type Response = {};", outbound(response))),
    ];
    let mut source = source;
    for (old, new) in replacements {
        assert_eq!(source.matches(&old).count(), 1, "pinned generated service changed: {old}");
        source = source.replace(&old, &new);
    }
    fs::write(path, source).expect("write adapted service");
    budget_generated_messages(path);
}

pub fn budget_generated_messages(path: &Path) {
    let source = fs::read_to_string(path).expect("read generated schema");
    // Emit capacity traversal adjacent to each generated struct/enum. This keeps
    // names in their original modules and follows nested Vec/Option/Box storage.
    let mut output = String::new();
    let mut item: Option<(String, usize, bool, Vec<String>)> = None;
    let mut services = false;
    for line in source.lines() {
        output.push_str(line);
        output.push('\n');
        services |= line == "/// Generated client implementations.";
        if services {
            continue;
        }
        let text = line.trim();
        let indent = line.len() - line.trim_start().len();
        if let Some((name, level, is_enum, fields)) = &mut item {
            if text == "}" && indent == *level {
                let body = if *is_enum {
                    if fields.is_empty() {
                        "0".to_owned()
                    } else {
                        format!(
                            "match self {{ {} }}",
                            fields
                                .iter()
                                .map(|field| format!(
                                    "Self::{field}(value) => kaspa_utils_tower::transport::RetainedSize::retained_heap_bytes(value),"
                                ))
                                .collect::<String>()
                        )
                    }
                } else {
                    fields.iter().fold("0u64".to_owned(), |body, field| {
                        format!(
                            "{body}.saturating_add(kaspa_utils_tower::transport::RetainedSize::retained_heap_bytes(&self.{field}))"
                        )
                    })
                };
                output.push_str(&format!("impl kaspa_utils_tower::transport::RetainedSize for {name} {{ fn retained_heap_bytes(&self) -> u64 {{ {body} }} }}\n"));
                item = None;
            } else if !*is_enum {
                if let Some(field) = text.strip_prefix("pub ").and_then(|s| s.split_once(':').map(|(field, _)| field)) {
                    fields.push(field.to_owned());
                }
            } else if !text.starts_with('#') && !text.starts_with("///") {
                if let Some((variant, _)) = text.split_once('(') {
                    fields.push(variant.to_owned());
                }
            }
        } else if let Some(name) = text.strip_prefix("pub struct ").and_then(|s| s.strip_suffix(" {")) {
            item = Some((name.to_owned(), indent, false, Vec::new()));
        } else if let Some(name) = text.strip_prefix("pub struct ").and_then(|s| s.strip_suffix(" {}")) {
            output.push_str(&format!(
                "impl kaspa_utils_tower::transport::RetainedSize for {name} {{ fn retained_heap_bytes(&self) -> u64 {{ 0 }} }}\n"
            ));
        } else if let Some(name) = text.strip_prefix("pub enum ").and_then(|s| s.strip_suffix(" {")) {
            item = Some((name.to_owned(), indent, true, Vec::new()));
        }
    }
    assert!(item.is_none(), "unterminated generated protobuf type");
    fs::write(path, build_wire_preflight::instrument_source(&output)).expect("write budgeted generated transport");
}
