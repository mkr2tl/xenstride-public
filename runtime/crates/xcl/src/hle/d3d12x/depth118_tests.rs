// Xenstride notice (GPL-2.0-only, section 2a): this file was added relative to ivall/xcl 9aec53d on 2026-11-23; see XENSTRIDE_CHANGES.md at the runtime root.
//! Logical D16+S8 fixture, not a guessed Xbox tiled byte layout or an automatic format remap.
use super::fixture_gpu::*;
use std::mem::transmute_copy;
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::Common::*;
fn encode(d:u16)->f32 { d as f32 / 65535.0 }
fn decode_canonical(d:f32)->Option<u16> {
    if !d.is_finite() || !(0.0..=1.0).contains(&d) {return None;}
    let q=(d*65535.0).round() as u16;
    (encode(q).to_bits()==d.to_bits()).then_some(q)
}
fn stencil(i:usize)->u8 { (i ^ (i>>8)) as u8 }
#[test]
fn p4_depth118_exhaustive_logical_storage_and_rejection_of_noncanonical_depth() {
    for depth in 0..=65535u16 {
        let encoded=encode(depth);assert_eq!(decode_canonical(encoded),Some(depth));
        for stencil in 0..=255u8 {
            let mut storage=[0u8;8];storage[..4].copy_from_slice(&encoded.to_le_bytes());storage[4]=stencil;
            assert_eq!(decode_canonical(f32::from_le_bytes(storage[..4].try_into().unwrap())),Some(depth));assert_eq!(storage[4],stencil);
            assert_eq!(&storage[5..],&[0,0,0]);
        }
    }
    for value in [f32::NAN,f32::INFINITY,-1.0,1.1,0.5] { assert_eq!(decode_canonical(value),None); }
}
#[test]
fn p4_depth118_warp_planes_srv_copy_roundtrip_and_dsv_clear() { unsafe {
    let mut g=Gpu::new();const W:usize=256;const H:usize=256;const N:usize=W*H;
    let desc=D3D12_RESOURCE_DESC {Dimension:D3D12_RESOURCE_DIMENSION_TEXTURE2D,Width:W as u64,Height:H as u32,DepthOrArraySize:1,MipLevels:1,
        Format:DXGI_FORMAT_R32G8X24_TYPELESS,SampleDesc:DXGI_SAMPLE_DESC {Count:1,Quality:0},Flags:D3D12_RESOURCE_FLAG_ALLOW_DEPTH_STENCIL,..Default::default()};
    let mut info=D3D12_FEATURE_DATA_FORMAT_INFO {Format:desc.Format,..Default::default()};
    g.dev.CheckFeatureSupport(D3D12_FEATURE_FORMAT_INFO,&mut info as *mut _ as _,std::mem::size_of_val(&info) as u32).unwrap();assert_eq!(info.PlaneCount,2);
    let a=g.resource(&desc,D3D12_HEAP_TYPE_DEFAULT,D3D12_RESOURCE_STATE_COPY_DEST);
    let b=g.resource(&desc,D3D12_HEAP_TYPE_DEFAULT,D3D12_RESOURCE_STATE_COPY_DEST);
    let mut fp=[D3D12_PLACED_SUBRESOURCE_FOOTPRINT::default();2];let mut rows=[0u32;2];let mut row_bytes=[0u64;2];let mut total=0;
    g.dev.GetCopyableFootprints(&desc,0,2,0,Some(fp.as_mut_ptr()),Some(rows.as_mut_ptr()),Some(row_bytes.as_mut_ptr()),Some(&mut total));
    assert_eq!(row_bytes,[W as u64*4,W as u64]);assert_eq!(rows,[H as u32;2]);
    let upload=g.buffer(total,D3D12_HEAP_TYPE_UPLOAD,D3D12_RESOURCE_STATE_GENERIC_READ,D3D12_RESOURCE_FLAG_NONE);
    let readback=g.buffer(total,D3D12_HEAP_TYPE_READBACK,D3D12_RESOURCE_STATE_COPY_DEST,D3D12_RESOURCE_FLAG_NONE);
    let output=g.buffer((N*8) as u64,D3D12_HEAP_TYPE_DEFAULT,D3D12_RESOURCE_STATE_UNORDERED_ACCESS,D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS);
    let output_rb=g.buffer((N*8) as u64,D3D12_HEAP_TYPE_READBACK,D3D12_RESOURCE_STATE_COPY_DEST,D3D12_RESOURCE_FLAG_NONE);
    let mapped=map(&upload);std::ptr::write_bytes(mapped,0,total as usize);
    for i in 0..N {
        let (x,y)=(i%W,i/W);
        (mapped.add(fp[0].Offset as usize+y*fp[0].Footprint.RowPitch as usize+4*x) as *mut f32).write_unaligned(encode(i as u16));
        *mapped.add(fp[1].Offset as usize+y*fp[1].Footprint.RowPitch as usize+x)=stencil(i);
    }
    upload.Unmap(0,None);
    let dsv:D3D12_DEPTH_STENCIL_VIEW_DESC=D3D12_DEPTH_STENCIL_VIEW_DESC {Format:DXGI_FORMAT_D32_FLOAT_S8X24_UINT,ViewDimension:D3D12_DSV_DIMENSION_TEXTURE2D,..Default::default()};
    let dsv_heap:ID3D12DescriptorHeap=g.dev.CreateDescriptorHeap(&D3D12_DESCRIPTOR_HEAP_DESC {Type:D3D12_DESCRIPTOR_HEAP_TYPE_DSV,NumDescriptors:1,..Default::default()}).unwrap();
    let dsv_handle=dsv_heap.GetCPUDescriptorHandleForHeapStart();g.dev.CreateDepthStencilView(&b,Some(&dsv),dsv_handle);
    let heap:ID3D12DescriptorHeap=g.dev.CreateDescriptorHeap(&D3D12_DESCRIPTOR_HEAP_DESC {Type:D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,NumDescriptors:2,Flags:D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE,..Default::default()}).unwrap();
    let increment=g.dev.GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV) as usize;
    for (plane,format) in [DXGI_FORMAT_R32_FLOAT_X8X24_TYPELESS,DXGI_FORMAT_X32_TYPELESS_G8X24_UINT].into_iter().enumerate() {
        let view=D3D12_SHADER_RESOURCE_VIEW_DESC {Format:format,ViewDimension:D3D12_SRV_DIMENSION_TEXTURE2D,Shader4ComponentMapping:D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
            Anonymous:D3D12_SHADER_RESOURCE_VIEW_DESC_0 {Texture2D:D3D12_TEX2D_SRV {MipLevels:1,PlaneSlice:plane as u32,..Default::default()}}};
        g.dev.CreateShaderResourceView(&b,Some(&view),D3D12_CPU_DESCRIPTOR_HANDLE {ptr:heap.GetCPUDescriptorHandleForHeapStart().ptr+plane*increment});
    }
    let range=D3D12_DESCRIPTOR_RANGE {RangeType:D3D12_DESCRIPTOR_RANGE_TYPE_SRV,NumDescriptors:2,..Default::default()};
    let params=[D3D12_ROOT_PARAMETER {ParameterType:D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE,Anonymous:D3D12_ROOT_PARAMETER_0 {
        DescriptorTable:D3D12_ROOT_DESCRIPTOR_TABLE {NumDescriptorRanges:1,pDescriptorRanges:&range}},ShaderVisibility:D3D12_SHADER_VISIBILITY_ALL},
        D3D12_ROOT_PARAMETER {ParameterType:D3D12_ROOT_PARAMETER_TYPE_UAV,Anonymous:D3D12_ROOT_PARAMETER_0 {Descriptor:D3D12_ROOT_DESCRIPTOR::default()},ShaderVisibility:D3D12_SHADER_VISIBILITY_ALL}];
    let rs=g.root(&params);
    let cs=compile("Texture2D<float> depth:register(t0); Texture2D<uint4> stencil:register(t1); RWStructuredBuffer<uint2> result:register(u0); [numthreads(8,8,1)] void main(uint3 id:SV_DispatchThreadID) { result[id.y*256+id.x]=uint2(asuint(depth.Load(int3(id.xy,0))),stencil.Load(int3(id.xy,0)).g); }",b"cs_5_0\0");
    let pso:ID3D12PipelineState=g.dev.CreateComputePipelineState(&D3D12_COMPUTE_PIPELINE_STATE_DESC {pRootSignature:transmute_copy(&Some(rs.clone())),CS:bytecode(&cs),..Default::default()}).unwrap();
    for plane in 0..2 {g.list.CopyTextureRegion(&texture_location(&a,plane),0,0,0,&buffer_location(&upload,fp[plane as usize]),None);}
    barrier(&g.list,&a,D3D12_RESOURCE_STATE_COPY_DEST,D3D12_RESOURCE_STATE_COPY_SOURCE);
    g.list.CopyResource(&b,&a);
    barrier(&g.list,&b,D3D12_RESOURCE_STATE_COPY_DEST,D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE);
    g.list.SetPipelineState(&pso);g.list.SetComputeRootSignature(&rs);g.list.SetDescriptorHeaps(&[Some(heap.clone())]);
    g.list.SetComputeRootDescriptorTable(0,heap.GetGPUDescriptorHandleForHeapStart());g.list.SetComputeRootUnorderedAccessView(1,output.GetGPUVirtualAddress());g.list.Dispatch(32,32,1);
    barrier(&g.list,&output,D3D12_RESOURCE_STATE_UNORDERED_ACCESS,D3D12_RESOURCE_STATE_COPY_SOURCE);
    g.list.CopyResource(&output_rb,&output);
    barrier(&g.list,&b,D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE,D3D12_RESOURCE_STATE_COPY_SOURCE);
    for plane in 0..2 {g.list.CopyTextureRegion(&buffer_location(&readback,fp[plane as usize]),0,0,0,&texture_location(&b,plane),None);}
    g.submit();
    let mapped=map(&readback);let srv=map(&output_rb);
    for i in 0..N {
        let (x,y)=(i%W,i/W);
        let d=(mapped.add(fp[0].Offset as usize+y*fp[0].Footprint.RowPitch as usize+4*x) as *const f32).read_unaligned();
        let s=*mapped.add(fp[1].Offset as usize+y*fp[1].Footprint.RowPitch as usize+x);
        assert_eq!(decode_canonical(d),Some(i as u16),"depth copy texel {i}");assert_eq!(s,stencil(i),"stencil copy {i}");
        assert_eq!((srv.add(i*8) as *const u32).read_unaligned(),encode(i as u16).to_bits(),"depth SRV {i}");
        assert_eq!((srv.add(i*8+4) as *const u32).read_unaligned(),stencil(i) as u32,"stencil SRV {i}");
    }
    readback.Unmap(0,None);output_rb.Unmap(0,None);
    g.reset();barrier(&g.list,&b,D3D12_RESOURCE_STATE_COPY_SOURCE,D3D12_RESOURCE_STATE_DEPTH_WRITE);
    let clear_depth=encode(32768);
    g.list.ClearDepthStencilView(dsv_handle,D3D12_CLEAR_FLAG_DEPTH|D3D12_CLEAR_FLAG_STENCIL,clear_depth,0xA5,None);
    barrier(&g.list,&b,D3D12_RESOURCE_STATE_DEPTH_WRITE,D3D12_RESOURCE_STATE_COPY_SOURCE);
    for plane in 0..2 {g.list.CopyTextureRegion(&buffer_location(&readback,fp[plane as usize]),0,0,0,&texture_location(&b,plane),None);}
    g.submit();let mapped=map(&readback);
    for i in 0..N {let(x,y)=(i%W,i/W);
        let d=(mapped.add(fp[0].Offset as usize+y*fp[0].Footprint.RowPitch as usize+4*x) as *const f32).read_unaligned();
        let s=*mapped.add(fp[1].Offset as usize+y*fp[1].Footprint.RowPitch as usize+x);
        assert_eq!(d.to_bits(),clear_depth.to_bits(),"DSV depth {i}");assert_eq!(s,0xA5,"DSV stencil {i}");
    }
    readback.Unmap(0,None);
    if let Some(path)=std::env::var_os("P4_DEPTH_OUTPUT") {
        std::fs::write(path,serde_json::to_vec_pretty(&serde_json::json!({"adapter":"explicit WARP","logical_source":"D16_UNORM plus S8_UINT; no Xbox tiling assumed", "storage_candidate":"R32G8X24_TYPELESS", "depth_values_checked":65536,"stencil_values_checked":256,
            "native_srv_texels_checked":65536,"native_copy_texels_checked":65536,"dsv_clear_texels_checked":65536,"automatic_runtime_remap":false})).unwrap()).unwrap();
    }
} }

/// Upstream 38b05b7 maps the console D16S8 family (118 depth, 119 depth view, 120 stencil view)
/// to the PC D24S8 one through `device_x::pc_resource_desc` / `pc_view_format` /
/// `pc_shader_view_format`. This runs that mapping on explicit WARP for every D16 value and every
/// stencil value: written through a copy, read back by copy and by the depth / stencil SRVs the
/// mapping picks, and a DSV clear. D16 semantics hold if each depth comes back as the same 16-bit
/// value once rounded to 16 bits (D24 has more precision; the stored bits differ).
#[test]
fn p6_upstream_d16s8_as_d24s8_keeps_every_depth_and_stencil_value_on_warp() { unsafe {
    use super::device_x::{pc_resource_desc, pc_shader_view_format, pc_view_format};
    const W: usize = 256; const H: usize = 256; const N: usize = W * H;
    // the console desc: TEXTURE2D 256x256, format 118, ALLOW_DEPTH_STENCIL
    let mut x = [0u8; 0x38];
    x[0..4].copy_from_slice(&3u32.to_le_bytes());
    x[0x10..0x18].copy_from_slice(&(W as u64).to_le_bytes());
    x[0x18..0x1C].copy_from_slice(&(H as u32).to_le_bytes());
    x[0x1C..0x1E].copy_from_slice(&1u16.to_le_bytes());
    x[0x1E..0x20].copy_from_slice(&1u16.to_le_bytes());
    x[0x20..0x24].copy_from_slice(&118u32.to_le_bytes());
    x[0x24..0x28].copy_from_slice(&1u32.to_le_bytes());
    x[0x30..0x34].copy_from_slice(&2u32.to_le_bytes());
    let pc = pc_resource_desc(&x);
    let desc: D3D12_RESOURCE_DESC = std::mem::transmute(pc);
    assert_eq!(desc.Format, DXGI_FORMAT_R24G8_TYPELESS);
    let (dsv_format, depth_view, stencil_view) = (pc_view_format(118), pc_shader_view_format(44, 119), pc_shader_view_format(44, 120));
    assert_eq!((dsv_format, depth_view, stencil_view), (DXGI_FORMAT_D24_UNORM_S8_UINT.0 as u32, DXGI_FORMAT_R24_UNORM_X8_TYPELESS.0 as u32, DXGI_FORMAT_X24_TYPELESS_G8_UINT.0 as u32));
    let d24 = |v: u16| ((v as u64 * 0xFF_FFFF + 0x7FFF) / 0xFFFF) as u32; // nearest D24 of a D16
    let back16 = |f: f32| (f * 65535.0).round() as u32;

    let mut g = Gpu::new();
    let a = g.resource(&desc, D3D12_HEAP_TYPE_DEFAULT, D3D12_RESOURCE_STATE_COPY_DEST);
    let b = g.resource(&desc, D3D12_HEAP_TYPE_DEFAULT, D3D12_RESOURCE_STATE_COPY_DEST);
    let mut fp = [D3D12_PLACED_SUBRESOURCE_FOOTPRINT::default(); 2];
    let mut total = 0;
    g.dev.GetCopyableFootprints(&desc, 0, 2, 0, Some(fp.as_mut_ptr()), None, None, Some(&mut total));
    let upload = g.buffer(total, D3D12_HEAP_TYPE_UPLOAD, D3D12_RESOURCE_STATE_GENERIC_READ, D3D12_RESOURCE_FLAG_NONE);
    let readback = g.buffer(total, D3D12_HEAP_TYPE_READBACK, D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_FLAG_NONE);
    let output = g.buffer((N * 8) as u64, D3D12_HEAP_TYPE_DEFAULT, D3D12_RESOURCE_STATE_UNORDERED_ACCESS, D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS);
    let output_rb = g.buffer((N * 8) as u64, D3D12_HEAP_TYPE_READBACK, D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_FLAG_NONE);
    let m = map(&upload);
    std::ptr::write_bytes(m, 0, total as usize);
    for i in 0..N {
        let (x, y) = (i % W, i / W);
        (m.add(fp[0].Offset as usize + y * fp[0].Footprint.RowPitch as usize + 4 * x) as *mut u32).write_unaligned(d24(i as u16));
        *m.add(fp[1].Offset as usize + y * fp[1].Footprint.RowPitch as usize + x) = stencil(i);
    }
    upload.Unmap(0, None);
    let dsv = D3D12_DEPTH_STENCIL_VIEW_DESC { Format: DXGI_FORMAT(dsv_format as i32), ViewDimension: D3D12_DSV_DIMENSION_TEXTURE2D, ..Default::default() };
    let dsv_heap: ID3D12DescriptorHeap = g.dev.CreateDescriptorHeap(&D3D12_DESCRIPTOR_HEAP_DESC { Type: D3D12_DESCRIPTOR_HEAP_TYPE_DSV, NumDescriptors: 1, ..Default::default() }).unwrap();
    let dsv_handle = dsv_heap.GetCPUDescriptorHandleForHeapStart();
    g.dev.CreateDepthStencilView(&b, Some(&dsv), dsv_handle);
    let heap: ID3D12DescriptorHeap = g.dev.CreateDescriptorHeap(&D3D12_DESCRIPTOR_HEAP_DESC { Type: D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV, NumDescriptors: 2, Flags: D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE, ..Default::default() }).unwrap();
    let inc = g.dev.GetDescriptorHandleIncrementSize(D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV) as usize;
    for (plane, format) in [depth_view, stencil_view].into_iter().enumerate() {
        let view = D3D12_SHADER_RESOURCE_VIEW_DESC { Format: DXGI_FORMAT(format as i32), ViewDimension: D3D12_SRV_DIMENSION_TEXTURE2D, Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
            Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 { Texture2D: D3D12_TEX2D_SRV { MipLevels: 1, PlaneSlice: plane as u32, ..Default::default() } } };
        g.dev.CreateShaderResourceView(&b, Some(&view), D3D12_CPU_DESCRIPTOR_HANDLE { ptr: heap.GetCPUDescriptorHandleForHeapStart().ptr + plane * inc });
    }
    let range = D3D12_DESCRIPTOR_RANGE { RangeType: D3D12_DESCRIPTOR_RANGE_TYPE_SRV, NumDescriptors: 2, ..Default::default() };
    let params = [D3D12_ROOT_PARAMETER { ParameterType: D3D12_ROOT_PARAMETER_TYPE_DESCRIPTOR_TABLE, Anonymous: D3D12_ROOT_PARAMETER_0 {
        DescriptorTable: D3D12_ROOT_DESCRIPTOR_TABLE { NumDescriptorRanges: 1, pDescriptorRanges: &range } }, ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL },
        D3D12_ROOT_PARAMETER { ParameterType: D3D12_ROOT_PARAMETER_TYPE_UAV, Anonymous: D3D12_ROOT_PARAMETER_0 { Descriptor: D3D12_ROOT_DESCRIPTOR::default() }, ShaderVisibility: D3D12_SHADER_VISIBILITY_ALL }];
    let rs = g.root(&params);
    let cs = compile("Texture2D<float> depth:register(t0); Texture2D<uint4> stencil:register(t1); RWStructuredBuffer<uint2> result:register(u0); [numthreads(8,8,1)] void main(uint3 id:SV_DispatchThreadID) { result[id.y*256+id.x]=uint2(asuint(depth.Load(int3(id.xy,0))),stencil.Load(int3(id.xy,0)).g); }", b"cs_5_0\0");
    let pso: ID3D12PipelineState = g.dev.CreateComputePipelineState(&D3D12_COMPUTE_PIPELINE_STATE_DESC { pRootSignature: transmute_copy(&Some(rs.clone())), CS: bytecode(&cs), ..Default::default() }).unwrap();
    for plane in 0..2 { g.list.CopyTextureRegion(&texture_location(&a, plane), 0, 0, 0, &buffer_location(&upload, fp[plane as usize]), None); }
    barrier(&g.list, &a, D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_COPY_SOURCE);
    g.list.CopyResource(&b, &a);
    barrier(&g.list, &b, D3D12_RESOURCE_STATE_COPY_DEST, D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE);
    g.list.SetPipelineState(&pso); g.list.SetComputeRootSignature(&rs); g.list.SetDescriptorHeaps(&[Some(heap.clone())]);
    g.list.SetComputeRootDescriptorTable(0, heap.GetGPUDescriptorHandleForHeapStart()); g.list.SetComputeRootUnorderedAccessView(1, output.GetGPUVirtualAddress()); g.list.Dispatch(32, 32, 1);
    barrier(&g.list, &output, D3D12_RESOURCE_STATE_UNORDERED_ACCESS, D3D12_RESOURCE_STATE_COPY_SOURCE);
    g.list.CopyResource(&output_rb, &output);
    barrier(&g.list, &b, D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE, D3D12_RESOURCE_STATE_COPY_SOURCE);
    for plane in 0..2 { g.list.CopyTextureRegion(&buffer_location(&readback, fp[plane as usize]), 0, 0, 0, &texture_location(&b, plane), None); }
    g.submit();
    let rb = map(&readback); let srv = map(&output_rb);
    for i in 0..N {
        let (x, y) = (i % W, i / W);
        let d = (rb.add(fp[0].Offset as usize + y * fp[0].Footprint.RowPitch as usize + 4 * x) as *const u32).read_unaligned() & 0xFF_FFFF;
        let s = *rb.add(fp[1].Offset as usize + y * fp[1].Footprint.RowPitch as usize + x);
        assert_eq!((d, s), (d24(i as u16), stencil(i)), "copy texel {i}");
        let f = f32::from_bits((srv.add(i * 8) as *const u32).read_unaligned());
        assert_eq!(back16(f), i as u32, "depth SRV {i}: {f}");
        assert_eq!((srv.add(i * 8 + 4) as *const u32).read_unaligned(), stencil(i) as u32, "stencil SRV {i}");
    }
    readback.Unmap(0, None); output_rb.Unmap(0, None);
    // DSV clear with a D16 value
    g.reset();
    barrier(&g.list, &b, D3D12_RESOURCE_STATE_COPY_SOURCE, D3D12_RESOURCE_STATE_DEPTH_WRITE);
    let clear = encode(32768);
    g.list.ClearDepthStencilView(dsv_handle, D3D12_CLEAR_FLAG_DEPTH | D3D12_CLEAR_FLAG_STENCIL, clear, 0xA5, None);
    barrier(&g.list, &b, D3D12_RESOURCE_STATE_DEPTH_WRITE, D3D12_RESOURCE_STATE_COPY_SOURCE);
    for plane in 0..2 { g.list.CopyTextureRegion(&buffer_location(&readback, fp[plane as usize]), 0, 0, 0, &texture_location(&b, plane), None); }
    g.submit();
    let rb = map(&readback);
    for i in 0..N {
        let (x, y) = (i % W, i / W);
        let d = (rb.add(fp[0].Offset as usize + y * fp[0].Footprint.RowPitch as usize + 4 * x) as *const u32).read_unaligned() & 0xFF_FFFF;
        let s = *rb.add(fp[1].Offset as usize + y * fp[1].Footprint.RowPitch as usize + x);
        assert_eq!(back16(d as f32 / 16777215.0), 32768, "DSV clear depth {i}: {d:#x}");
        assert_eq!(s, 0xA5, "DSV clear stencil {i}");
    }
    readback.Unmap(0, None);
} }
