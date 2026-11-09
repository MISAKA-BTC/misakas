//! Parse generated schemas as Rust AST, never infer memory from wire length alone.
use quote::{ToTokens, quote};
use syn::{Attribute, Expr, Fields, GenericArgument, Item, Lit, Meta, PathArguments, Token, Type, punctuated::Punctuated};

#[derive(Default)]
struct Descriptor {
    kind: String,
    tag: Option<u32>,
    tags: Vec<u32>,
    repeated: bool,
    oneof: Option<Type>,
}
fn descriptor(attrs: &[Attribute]) -> Descriptor {
    let mut result = Descriptor::default();
    for attr in attrs.iter().filter(|a| a.path().is_ident("prost")) {
        let entries = attr.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated).expect("prost field descriptor");
        for entry in entries {
            match entry {
                Meta::Path(path) => {
                    let name = path.get_ident().expect("simple prost descriptor").to_string();
                    match name.as_str() {
                        "repeated" => result.repeated = true,
                        "optional" | "required" | "boxed" => {}
                        _ => result.kind = name,
                    }
                }
                Meta::NameValue(value) => {
                    let name = value.path.get_ident().expect("simple prost descriptor").to_string();
                    let Expr::Lit(value) = value.value else { panic!("literal prost descriptor") };
                    let Lit::Str(value) = value.lit else { panic!("string prost descriptor") };
                    match name.as_str() {
                        "tag" => result.tag = Some(value.value().parse().expect("prost numeric tag")),
                        "tags" => {
                            result.tags = value.value().split(',').map(|s| s.trim().parse().expect("prost numeric tag")).collect()
                        }
                        "oneof" => result.oneof = Some(syn::parse_str(&value.value()).expect("prost oneof path")),
                        "bytes" | "enumeration" => result.kind = name,
                        "packed" | "default" => {}
                        _ => panic!("unsupported prost memory descriptor {name}"),
                    }
                }
                Meta::List(_) => panic!("unsupported nested prost descriptor"),
            }
        }
    }
    result
}
fn derive(attrs: &[Attribute], name: &str) -> bool {
    attrs.iter().filter(|a| a.path().is_ident("derive")).any(|attr| {
        attr.parse_args_with(Punctuated::<syn::Path, Token![,]>::parse_terminated)
            .unwrap()
            .iter()
            .any(|path| path.segments.last().unwrap().ident == name && path.segments.iter().any(|s| s.ident == "prost"))
    })
}
fn peel(mut ty: &Type, repeated: bool) -> (&Type, bool) {
    let mut boxed = false;
    loop {
        let Type::Path(path) = ty else { panic!("unsupported generated field type") };
        let last = path.path.segments.last().unwrap();
        if last.ident == "Option" || last.ident == "Box" || repeated && last.ident == "Vec" {
            boxed |= last.ident == "Box";
            let PathArguments::AngleBracketed(args) = &last.arguments else { panic!("prost type arguments") };
            let Some(GenericArgument::Type(next)) = args.args.first() else { panic!("prost type argument") };
            ty = next;
            // Only peel the outer repeated Vec. bytes elements are themselves Vec<u8>.
            if last.ident == "Vec" {
                return (ty, boxed);
            }
        } else {
            return (ty, boxed);
        }
    }
}
fn field_body(desc: &Descriptor, ty: &Type, state: proc_macro2::TokenStream) -> proc_macro2::TokenStream {
    let (element, boxed) = peel(ty, desc.repeated);
    let q = quote!(kaspa_utils_tower::wire_preflight);
    let array = if desc.repeated { quote!(estimate.unmerged.array::<#element>(1, #state)?;) } else { quote!() };
    match desc.kind.as_str() {
        "bytes" | "string" => {
            let utf8 = if desc.kind == "string" {
                quote!(std::str::from_utf8(value).map_err(|_| #q::PreflightError("invalid protobuf UTF-8"))?;)
            } else {
                quote!()
            };
            let blob_state = if desc.repeated { quote!(&mut #q::FieldState::default()) } else { state };
            // Prost constructs the new heap-backed value before pushing it.
            // Its bytes can coexist with both Vec backing allocations.
            quote! { let value = cursor.bytes(wire)?; #utf8 estimate.unmerged.blob(value.len(), #blob_state)?; #array }
        }
        "message" => {
            let add_child =
                if desc.repeated { quote!(estimate.child(child)?;) } else { quote!(estimate.grouped_child(child, #state)?;) };
            // For Vec<Box<M>>, the array stores pointers and each child owns M.
            let (inner, inner_boxed) = peel(element, false);
            let box_bytes = if boxed || inner_boxed {
                quote!(estimate.unmerged.owned(std::mem::size_of::<#inner>() as u64, std::mem::size_of::<#inner>() as u64)?;)
            } else {
                quote!()
            };
            quote! {
                let value = cursor.bytes(wire)?;
                let child = <#inner as #q::WirePreflight>::wire_fragment_bound(value, depth.checked_add(1).ok_or(#q::PreflightError("protobuf recursion overflow"))?)?;
                #box_bytes #add_child #array
            }
        }
        kind => {
            let wire = match kind {
                "double" | "fixed64" | "sfixed64" => quote!(#q::WireType::Fixed64),
                "float" | "fixed32" | "sfixed32" => quote!(#q::WireType::Fixed32),
                "bool" | "uint32" | "uint64" | "int32" | "int64" | "sint32" | "sint64" | "enumeration" => quote!(#q::WireType::Varint),
                _ => panic!("unsupported prost memory field kind {kind}"),
            };
            if desc.repeated {
                quote! { let count = cursor.scalar_count(wire, #wire, depth)?; estimate.unmerged.array::<#element>(count, #state)?; }
            } else {
                quote!(cursor.scalar(wire, #wire)?;)
            }
        }
    }
}

fn instrument(items: &mut Vec<Item>) {
    let mut additions = Vec::new();
    for item in items.iter_mut() {
        match item {
            Item::Mod(module) => {
                if let Some((_, inner)) = &mut module.content {
                    instrument(inner);
                }
            }
            Item::Struct(message) if derive(&message.attrs, "Message") => {
                let name = &message.ident;
                let fields: Vec<_> = message.fields.iter().collect();
                let slots = fields.len();
                let mut cases = Vec::new();
                for (index, field) in fields.iter().enumerate() {
                    let desc = descriptor(&field.attrs);
                    if let Some(oneof) = desc.oneof {
                        let tags = desc.tags;
                        assert!(!tags.is_empty(), "oneof must list its wire tags");
                        cases.push(quote!(#(#tags)|* => {
                            let fragment = <#oneof as kaspa_utils_tower::wire_preflight::OneofPreflight>::wire_field_fragment(tag, wire, &mut cursor, depth)?;
                            estimate.oneof_child(fragment, &mut states[#index], tag)?;
                        },));
                    } else {
                        let tag = desc.tag.expect("prost field tag");
                        let body = field_body(&desc, &field.ty, quote!(&mut states[#index]));
                        cases.push(quote!(#tag => { #body },));
                    }
                }
                let generated = quote! {
                    #[allow(unused_mut, unused_variables)]
                    impl kaspa_utils_tower::wire_preflight::WirePreflight for #name {
                        fn wire_fragment_bound(bytes: &[u8], depth: usize) -> kaspa_utils_tower::wire_preflight::Result<kaspa_utils_tower::wire_preflight::FragmentBound> {
                            let mut cursor = kaspa_utils_tower::wire_preflight::Cursor::new(bytes, depth)?;
                            let mut estimate = kaspa_utils_tower::wire_preflight::FragmentBound::default();
                            let mut states = [kaspa_utils_tower::wire_preflight::FieldState::default(); #slots];
                            while let Some((tag, wire)) = cursor.key()? {
                                match tag { #(#cases)* _ => cursor.skip(tag, wire, depth)?, }
                            }
                            estimate.finish_fields(&states)
                        }
                    }
                };
                additions.push(syn::parse2::<Item>(generated).expect("generated message preflight"));
            }
            Item::Enum(oneof) if derive(&oneof.attrs, "Oneof") => {
                let name = &oneof.ident;
                let mut cases = Vec::new();
                for variant in &oneof.variants {
                    let desc = descriptor(&variant.attrs);
                    let tag = desc.tag.expect("prost oneof tag");
                    let Fields::Unnamed(fields) = &variant.fields else { panic!("prost oneof tuple") };
                    assert_eq!(fields.unnamed.len(), 1);
                    let body = field_body(&desc, &fields.unnamed.first().unwrap().ty, quote!(&mut state));
                    cases.push(quote!(#tag => { #body },));
                }
                let generated = quote! {
                    #[allow(unused_mut, unused_variables)]
                    impl kaspa_utils_tower::wire_preflight::OneofPreflight for #name {
                        fn wire_field_fragment(tag: u32, wire: kaspa_utils_tower::wire_preflight::WireType, cursor: &mut kaspa_utils_tower::wire_preflight::Cursor<'_>, depth: usize) -> kaspa_utils_tower::wire_preflight::Result<kaspa_utils_tower::wire_preflight::FragmentBound> {
                            let mut estimate = kaspa_utils_tower::wire_preflight::FragmentBound::default();
                            let mut state = kaspa_utils_tower::wire_preflight::FieldState::default();
                            match tag { #(#cases)* _ => return Err(kaspa_utils_tower::wire_preflight::PreflightError("unknown oneof schema tag")), }
                            estimate.finish_fields(&[state])
                        }
                    }
                };
                additions.push(syn::parse2::<Item>(generated).expect("generated oneof preflight"));
            }
            _ => {}
        }
    }
    items.extend(additions);
}
pub fn instrument_source(source: &str) -> String {
    let mut file = syn::parse_file(source).expect("parse fixed generated protobuf AST");
    instrument(&mut file.items);
    file.into_token_stream().to_string()
}
