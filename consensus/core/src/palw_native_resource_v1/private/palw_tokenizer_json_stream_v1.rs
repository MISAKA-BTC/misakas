// Generated owner-local field visitor. Root/hash/codec unchanged.
#[allow(unused_imports)]
use crate::palw_native_resource_v1::{NativeHeapV1, NativeCoverageV1, native_struct, native_tuple_struct, native_enum, native_unit};
native_struct!(crate::palw_tokenizer_json_stream_v1::PalwTokenizerJsonStreamV1; context, tokenizer, length, offset, target, hash, stack, lex, keys, key_events, work_overrun, pending, next_object, started, root_done, invalid, sealed, highest, chosen, added_chosen);
native_struct!(crate::palw_tokenizer_json_stream_v1::Frame; object, scope, namespace, phase, field, flags, items, candidate, added_id, added_content);
native_enum!(crate::palw_tokenizer_json_stream_v1::Lex; Idle, String(f0), Number(f0), Keyword { kind, index });
native_enum!(crate::palw_tokenizer_json_stream_v1::Scope; Root, Model, Vocab, Merges, MergePair, AddedList, AddedEntry, Ignore);
native_enum!(crate::palw_tokenizer_json_stream_v1::Field; Ignore, Model, Vocab, Merges, Added, Id, Content);
native_struct!(crate::palw_tokenizer_json_stream_v1::StringState; key, word, long_word, utf8, escape, hex, hex_digits, high, key_hash, ordinary, literal, ordinary_len, literal_len, non_byte, space);
native_struct!(crate::palw_tokenizer_json_stream_v1::NumberState; phase, positive, value, float, integer_digits, leading_zeros, significant, seen_nonzero, exponent, exponent_positive);
