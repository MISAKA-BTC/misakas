# glibc 2.39 transcription sources (ADR-0031)

Verbatim copies of the exact sources `consensus/core/src/palw_transcendental.rs` transcribes,
fetched 2026-08-16 from the `glibc-2.39` release tag of the bminor/glibc GitHub mirror
(`sysdeps/ieee754/flt-32/`). Kept so a reviewer can diff the transcription against its source
without trusting the network. These files are LGPL-2.1-or-later (headers intact); they are
REFERENCE MATERIAL ONLY and are never compiled into any MISAKA binary.

The fleet's libm dispatches the FMA multiarch builds of these files
(`sysdeps/x86_64/fpu/multiarch/e_expf-fma.c` = the same C recompiled with `-mfma`); which
contraction variant a class binds is a registration-time disassembly fact (ADR-0031 Fact 2).
