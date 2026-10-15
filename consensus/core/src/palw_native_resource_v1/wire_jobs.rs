//! Exact manual versioned FP job codec, generated from owner field order.
use super::*;
fn scan_fp_job_body_v1<R:std::io::Read>(reader:&mut NativeWireReaderV1<R>,version:u16)->std::io::Result<NativeWireShapeV1>{let mut sum=NativeWireShapeV1::default();sum=sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;sum=sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;sum=sum.merge(<crate::tx::TransactionOutpoint as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;sum=sum.merge(<Vec<u8> as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;sum=sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;sum=sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;sum=sum.merge(<u64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;sum=sum.merge(<[u8; 32] as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;sum=sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;sum=sum.merge(<crate::Hash64 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;sum=sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;sum=sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;sum=sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;sum=sum.merge(<u8 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;sum=sum.merge(<u8 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;sum=sum.merge(<[u8; 32] as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;sum=sum.merge(<u32 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;
 let v5=crate::palw_fp_job_v5::PALW_FP_V5_VERSION;
 let eval=crate::palw_improve_eval_v1::PALW_FP_EVAL_VERSION;
 let prefix=crate::palw_fp_prefix_v1::PALW_FP_PREFIX_VERSION;
 let inherit=crate::palw_fp_prefix_v1::PALW_FP_PREFIX_INHERIT_VERSION;
 if [crate::palw_freeprompt_v3::PALW_FP_V4_VERSION,v5,eval,prefix,inherit].contains(&version) {sum=sum.merge(<crate::palw_decode_pipeline_v4::DecodeConfigV4 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;}
 if version==prefix || version==inherit {sum=sum.merge(<crate::palw_freeprompt_v3::PalwFpPrefixStateV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;}
 if version==crate::palw_fp_constraint_job_v1::PALW_FP_CONSTRAINT_VERSION {sum=sum.merge(<crate::palw_fp_constraint_job_v1::PalwFpConstraintTailV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;}
 if version==v5 {sum=sum.merge(<crate::palw_fp_job_v5::PalwFpV5TailV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;}
 if version==eval {sum=sum.merge(<Box<crate::palw_improve_eval_v1::PalwEvalJobV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;}
 if version==crate::palw_gen_claim_v1::PALW_FP_GEN_VERSION {sum=sum.merge(<Box<crate::palw_gen_claim_v1::PalwGenJobTailV1> as NativeWireScanV1>::scan_native_wire_v1(reader)?)?;}
 Ok(sum)
}
impl NativeWireScanV1 for crate::palw_freeprompt_v3::PalwFreePromptJobV3 {
 fn scan_native_wire_v1<R:std::io::Read>(reader:&mut NativeWireReaderV1<R>)->std::io::Result<NativeWireShapeV1>{reader.step()?;let version=reader.short()?;scan_fp_job_body_v1(reader,version)}
}
impl NativeWireScanV1 for crate::palw_fp_job_v5::PalwFreePromptJobV5 {
 fn scan_native_wire_v1<R:std::io::Read>(reader:&mut NativeWireReaderV1<R>)->std::io::Result<NativeWireShapeV1>{reader.step()?;if reader.short()?!=crate::palw_fp_job_v5::PALW_FP_V5_VERSION {return Err(wire::wire_bad("wrong FP V5 job version"));}
 let sum=scan_fp_job_body_v1(reader,crate::palw_freeprompt_v3::PALW_FP_V4_VERSION)?;
 sum.merge(<crate::palw_fp_job_v5::PalwFpV5TailV1 as NativeWireScanV1>::scan_native_wire_v1(reader)?)
 }
}
