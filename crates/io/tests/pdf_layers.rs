// The `a_saved_model_lists_its_arch_layers` test lives in the engine crate
// (crates/engine/tests/arch_layers_dxf.rs) because it needs both `Session`
// (L4) and `cadcraft_io` (L3). The io crate (L3) cannot depend on engine (L4).
