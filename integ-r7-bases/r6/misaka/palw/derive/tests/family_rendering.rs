//! **The fused and unfused A16 rows render identically, asserted rather than assumed.**
//!
//! `rendered_output_hash_for_family` maps both to one call. That is correct — the fusion replaces
//! attention's scores/softmax/values nodes with one fused node and changes no output ids, and
//! rendering is a property of the tokenizer and vocabulary — but "correct because I reasoned it"
//! is what a shared match arm looks like when it is wrong. This is the check.

use misaka_palw_base0::e2e_drill::PalwRcFamilyV1;
use misaka_palw_derive::derive::rendered_output_hash_for_family;

#[test]
fn the_fused_family_renders_as_the_unfused_one() {
    for ids in [&[][..], &[1u32][..], &[3, 5, 8, 13, 21][..], &[151643, 872, 15339][..]] {
        assert_eq!(
            rendered_output_hash_for_family(PalwRcFamilyV1::Qwen25A16, ids),
            rendered_output_hash_for_family(PalwRcFamilyV1::Qwen25A16V5, ids),
            "the fused and unfused A16 rows rendered {ids:?} differently — the fusion changed the OUTPUT, not just \
             the graph, and every derivation taken under one row is unreproducible under the other"
        );
    }
    // And not vacuous: a different family really does render differently.
    assert_ne!(
        rendered_output_hash_for_family(PalwRcFamilyV1::Qwen25A16V5, &[3, 5, 8]),
        rendered_output_hash_for_family(PalwRcFamilyV1::Base0, &[3, 5, 8]),
        "every family renders the same bytes, so this test could not detect a family that did not"
    );
}
