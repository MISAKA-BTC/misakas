//! Private read-only geometry shared by exact evaluation; not a width/admission capability.
use super::*;
use crate::palw_tir_step_v1::{
    PalwTirStateInstanceV1,
    wide_v2::{PalwTirStepSpaceV2, PalwTirVerifiedBindingV2},
};
pub(super) trait ExactGeometry {
    fn program(&self) -> &TirProgramV1;
    fn info(&self) -> &misaka_palw_tir::validate::ProgramInfo;
    fn layout(&self) -> &crate::palw_tir_class_v1::PalwTirLayoutV1;
    fn occurrences(&self) -> &[(u8, Option<u16>)];
    fn program_slots(&self) -> u32;
    fn node_slot(&self, occurrence: usize, node: u16) -> Option<u32>;
    fn commit_tile_len(&self, block: u8, node: u16) -> Option<u32>;
    fn fixed_instance_index(&self, state: u16, layer: Option<u16>) -> Option<usize>;
    fn hist_instance_index(&self, state: u16, layer: Option<u16>) -> Option<usize>;
    fn fixed_instances(&self) -> &[PalwTirStateInstanceV1];
    fn hist_instances(&self) -> &[PalwTirStateInstanceV1];
    fn leaf_index(&self, ctx: &PalwJobContextV2, coord: &PalwStepCoordinateV1) -> Option<u64>;
    fn leaf_at(&self, ctx: &PalwJobContextV2, index: u64) -> Option<PalwTirLeafV1>;
}
macro_rules! shared {
    ($ty:ty) => {
        fn occurrences(&self) -> &[(u8, Option<u16>)] {
            <$ty>::occurrences(self)
        }
        fn program_slots(&self) -> u32 {
            <$ty>::program_slots(self)
        }
        fn node_slot(&self, o: usize, n: u16) -> Option<u32> {
            <$ty>::node_slot(self, o, n)
        }
        fn commit_tile_len(&self, b: u8, n: u16) -> Option<u32> {
            <$ty>::commit_tile_len(self, b, n)
        }
        fn fixed_instance_index(&self, s: u16, l: Option<u16>) -> Option<usize> {
            <$ty>::fixed_instance_index(self, s, l)
        }
        fn hist_instance_index(&self, s: u16, l: Option<u16>) -> Option<usize> {
            <$ty>::hist_instance_index(self, s, l)
        }
        fn fixed_instances(&self) -> &[PalwTirStateInstanceV1] {
            <$ty>::fixed_instances(self)
        }
        fn hist_instances(&self) -> &[PalwTirStateInstanceV1] {
            <$ty>::hist_instances(self)
        }
        fn leaf_index(&self, c: &PalwJobContextV2, p: &PalwStepCoordinateV1) -> Option<u64> {
            <$ty>::leaf_index(self, c, p)
        }
        fn leaf_at(&self, c: &PalwJobContextV2, i: u64) -> Option<PalwTirLeafV1> {
            <$ty>::leaf_at(self, c, i)
        }
    };
}
impl ExactGeometry for PalwTirStepSpaceV1 {
    fn program(&self) -> &TirProgramV1 {
        &self.program
    }
    fn info(&self) -> &misaka_palw_tir::validate::ProgramInfo {
        &self.info
    }
    fn layout(&self) -> &crate::palw_tir_class_v1::PalwTirLayoutV1 {
        &self.layout
    }
    shared!(PalwTirStepSpaceV1);
}
impl ExactGeometry for PalwTirStepSpaceV2 {
    fn program(&self) -> &TirProgramV1 {
        PalwTirStepSpaceV2::program(self)
    }
    fn info(&self) -> &misaka_palw_tir::validate::ProgramInfo {
        PalwTirStepSpaceV2::info(self)
    }
    fn layout(&self) -> &crate::palw_tir_class_v1::PalwTirLayoutV1 {
        PalwTirStepSpaceV2::layout(self)
    }
    shared!(PalwTirStepSpaceV2);
}
pub(super) trait ExactBindingGeometry {
    fn space(&self) -> &dyn ExactGeometry;
    fn class_id(&self) -> Hash64;
    fn context_hash(&self) -> Hash64;
}
impl ExactBindingGeometry for PalwTirVerifiedBindingV1 {
    fn space(&self) -> &dyn ExactGeometry {
        &self.space
    }
    fn class_id(&self) -> Hash64 {
        self.class_id
    }
    fn context_hash(&self) -> Hash64 {
        self.context_hash
    }
}
impl ExactBindingGeometry for PalwTirVerifiedBindingV2 {
    fn space(&self) -> &dyn ExactGeometry {
        PalwTirVerifiedBindingV2::space(self)
    }
    fn class_id(&self) -> Hash64 {
        PalwTirVerifiedBindingV2::class_id(self)
    }
    fn context_hash(&self) -> Hash64 {
        PalwTirVerifiedBindingV2::context_hash(self)
    }
}
impl<T: ExactBindingGeometry + ?Sized> ExactBindingGeometry for Box<T> {
    fn space(&self) -> &dyn ExactGeometry {
        (**self).space()
    }
    fn class_id(&self) -> Hash64 {
        (**self).class_id()
    }
    fn context_hash(&self) -> Hash64 {
        (**self).context_hash()
    }
}
