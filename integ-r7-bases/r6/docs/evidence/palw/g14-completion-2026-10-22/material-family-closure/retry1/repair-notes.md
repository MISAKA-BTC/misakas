# Preserve the actual SourceGraph absolute packet-ID representation

Initial compilation failed because two new metadata owners declared absolute packet IDs as Hash64, while NativeSourceMaterialGraphV3::absolute_packet_id_v3 returns the existing [u8;64] representation. Associated direct identity comparisons failed too. Initial added source, tracked patch, compiler log and hash manifest are preserved in the parent directory.

This retry changes only the PrefixBoundaryDependencyV1 and SupplierDependencyV1 absolute fields to [u8;64], matching the existing SourceGraph and the earlier Output/Selection metadata. No hash recomputation, byte conversion, public wire, native ID meaning, cap or assertion is changed. sizeof-based native metadata requests use the actual final type. Build/test success remains unproven until raw logs complete.
