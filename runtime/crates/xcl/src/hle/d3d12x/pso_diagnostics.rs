// Xenstride notice (GPL-2.0-only, section 2a): this file was added relative to ivall/xcl 9aec53d on 2026-11-23; see XENSTRIDE_CHANGES.md at the runtime root.
//! Bounded, scalar-only pipeline failure descriptions. No shader blobs or pointers.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::sync::Mutex;
use windows::Win32::Graphics::Direct3D12::*;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rejection {
    pub sequence: u64,
    pub thread: u32,
    pub uptime_us: u64,
    pub kind: String,
    pub stage: String,
    pub hresult: String,
    pub description: Value,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Snapshot {
    pub total_failed_attempts: u64,
    pub first: Vec<Rejection>,
    pub latest: VecDeque<Rejection>,
}
static RECORDS: Mutex<Snapshot> = Mutex::new(Snapshot {
    total_failed_attempts: 0,
    first: Vec::new(),
    latest: VecDeque::new(),
});
pub fn snapshot() -> Snapshot {
    RECORDS.try_lock().map(|r| r.clone()).unwrap_or_default()
}
fn record(kind: &str, stage: &str, hr: i32, description: Value) {
    if hr >= 0 {
        return;
    }
    let saved = unsafe { windows_sys::Win32::Foundation::GetLastError() };
    let mut records = RECORDS.lock().unwrap_or_else(|e| e.into_inner());
    records.total_failed_attempts += 1;
    let event = Rejection {
        sequence: records.total_failed_attempts,
        thread: unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() },
        uptime_us: (crate::log::uptime() * 1_000_000.0) as u64,
        kind: kind.into(),
        stage: stage.into(),
        hresult: format!("0x{:08x}", hr as u32),
        description,
    };
    if records.first.len() < 16 {
        records.first.push(event.clone());
    }
    if records.latest.len() == 16 {
        records.latest.pop_front();
    }
    records.latest.push_back(event.clone());
    drop(records);
    if event.sequence <= 8 || event.sequence.is_power_of_two() {
        if let Ok(json) = serde_json::to_string(&event) {
            crate::xlog_warn!("[pso-rejected] {json}");
        }
    }
    unsafe {
        windows_sys::Win32::Foundation::SetLastError(saved);
    }
}
fn face(d: &D3D12_DEPTH_STENCILOP_DESC) -> Value {
    json!({"fail":d.StencilFailOp.0,"depth_fail":d.StencilDepthFailOp.0,"pass":d.StencilPassOp.0,"func":d.StencilFunc.0})
}
fn depth(d: &D3D12_DEPTH_STENCIL_DESC) -> Value {
    json!({"enabled":d.DepthEnable.0,"write_mask":d.DepthWriteMask.0,"func":d.DepthFunc.0,
        "stencil_enabled":d.StencilEnable.0,"stencil_read_mask":d.StencilReadMask,"stencil_write_mask":d.StencilWriteMask,
        "front":face(&d.FrontFace),"back":face(&d.BackFace)})
}
fn raster(d: &D3D12_RASTERIZER_DESC) -> Value {
    json!({"fill":d.FillMode.0,"cull":d.CullMode.0,"front_ccw":d.FrontCounterClockwise.0,"depth_bias":d.DepthBias,
        "depth_bias_clamp_bits":d.DepthBiasClamp.to_bits(),"slope_bias_bits":d.SlopeScaledDepthBias.to_bits(),
        "depth_clip":d.DepthClipEnable.0,"multisample":d.MultisampleEnable.0,"aa_line":d.AntialiasedLineEnable.0,
        "forced_samples":d.ForcedSampleCount,"conservative":d.ConservativeRaster.0})
}
fn blend(d: &D3D12_BLEND_DESC) -> Value {
    json!({"alpha_to_coverage":d.AlphaToCoverageEnable.0,"independent":d.IndependentBlendEnable.0,
        "targets":d.RenderTarget.iter().map(|r|json!({"enabled":r.BlendEnable.0,"logic_enabled":r.LogicOpEnable.0,
            "src":r.SrcBlend.0,"dst":r.DestBlend.0,"op":r.BlendOp.0,"src_alpha":r.SrcBlendAlpha.0,
            "dst_alpha":r.DestBlendAlpha.0,"alpha_op":r.BlendOpAlpha.0,"logic_op":r.LogicOp.0,"write_mask":r.RenderTargetWriteMask})).collect::<Vec<_>>()})
}
fn shader(d: &D3D12_SHADER_BYTECODE) -> Value {
    json!({"present":!d.pShaderBytecode.is_null(),"bytes":d.BytecodeLength})
}
pub fn graphics(stage: &str, hr: i32, d: &D3D12_GRAPHICS_PIPELINE_STATE_DESC) {
    record(
        "graphics",
        stage,
        hr,
        json!({"topology_type":d.PrimitiveTopologyType.0,"rt_count":d.NumRenderTargets,
        "rt_formats":d.RTVFormats.map(|f|f.0),"dsv_format":d.DSVFormat.0,
        "pc_dsv_format":matches!(d.DSVFormat.0,0|20|40|45|55),
        "sample_count":d.SampleDesc.Count,"sample_quality":d.SampleDesc.Quality,"sample_mask":d.SampleMask,
        "rasterizer":raster(&d.RasterizerState),"depth_stencil":depth(&d.DepthStencilState),"blend":blend(&d.BlendState),
        "flags":d.Flags.0,"node_mask":d.NodeMask,"strip_cut":d.IBStripCutValue.0,
        "vs":shader(&d.VS),"ps":shader(&d.PS),"hs":shader(&d.HS),"ds":shader(&d.DS),"gs":shader(&d.GS),
        "input_elements":d.InputLayout.NumElements,"so_entries":d.StreamOutput.NumEntries,
        "so_strides":d.StreamOutput.NumStrides,"cached_pso_bytes":d.CachedPSO.CachedBlobSizeInBytes,
        "indirect_arrays_and_shader_contents_not_copied":true}),
    );
}
pub unsafe fn stream(stage: &str, hr: i32, desc: *const std::ffi::c_void) {
    let description = match super::real::stream_bytes(desc) {
        None => json!({"valid_outer_descriptor":false}),
        Some(bytes) => match super::real::stream_subobjects(bytes) {
            None => json!({"bytes":bytes.len(),"valid_structure":false}),
            Some(subs) => {
                let mut states = Vec::new();
                for (ty, p) in subs {
                    let value = match ty {
                        // Public scalar states only: pointers/padding are never serialized.
                        9 | 13 | 14 | 16 | 18 | 20 => {
                            json!({"value":u32::from_le_bytes(p[..4].try_into().unwrap())})
                        }
                        15 => {
                            json!({"formats":(0..8).map(|i|u32::from_le_bytes(p[4*i..4*i+4].try_into().unwrap())).collect::<Vec<_>>(),
                            "count":u32::from_le_bytes(p[32..36].try_into().unwrap())})
                        }
                        17 => {
                            json!({"count":u32::from_le_bytes(p[..4].try_into().unwrap()),"quality":u32::from_le_bytes(p[4..8].try_into().unwrap())})
                        }
                        8 => blend(&p.as_ptr().cast::<D3D12_BLEND_DESC>().read_unaligned()),
                        10 => raster(&p.as_ptr().cast::<D3D12_RASTERIZER_DESC>().read_unaligned()),
                        11 | 21 => depth(
                            &p.as_ptr()
                                .cast::<D3D12_DEPTH_STENCIL_DESC>()
                                .read_unaligned(),
                        ),
                        1..=6 | 24 | 25 => {
                            shader(&p.as_ptr().cast::<D3D12_SHADER_BYTECODE>().read_unaligned())
                        }
                        _ => json!({"payload_bytes":p.len(),"contents_not_copied":true}),
                    };
                    states.push(json!({"type":ty,"state":value}));
                }
                json!({"bytes":bytes.len(),"valid_structure":true,"subobjects":states})
            }
        },
    };
    record("stream", stage, hr, description);
}
