// Xenstride notice (GPL-2.0-only, section 2a): this file was added relative to ivall/xcl 9aec53d on 2026-11-23; see XENSTRIDE_CHANGES.md at the runtime root.
//! Native, explicit-WARP helpers for independent public-API fixtures.
use std::ffi::c_void;
use std::mem::{transmute_copy, ManuallyDrop};
use windows::core::{Interface, PCSTR};
use windows::Win32::Graphics::Direct3D::{Fxc::D3DCompile, ID3DBlob, D3D_FEATURE_LEVEL_11_0};
use windows::Win32::Graphics::Direct3D12::*;
use windows::Win32::Graphics::Dxgi::{Common::*, *};
pub struct Gpu { pub dev:ID3D12Device,pub queue:ID3D12CommandQueue,pub alloc:ID3D12CommandAllocator,pub list:ID3D12GraphicsCommandList,
    fence:ID3D12Fence,value:u64 }
impl Gpu {
    pub unsafe fn new()->Self {
        let factory:IDXGIFactory4=CreateDXGIFactory1().expect("DXGI");
        let adapter:IDXGIAdapter1=factory.EnumWarpAdapter().expect("WARP");
        let description=adapter.GetDesc1().unwrap();
        assert_ne!(description.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32,0);
        let mut dev=None;D3D12CreateDevice(&adapter,D3D_FEATURE_LEVEL_11_0,&mut dev).expect("explicit WARP device");let dev:ID3D12Device=dev.unwrap();
        let luid=dev.GetAdapterLuid();assert_eq!(luid.LowPart,description.AdapterLuid.LowPart);assert_eq!(luid.HighPart,description.AdapterLuid.HighPart);
        let queue=dev.CreateCommandQueue(&D3D12_COMMAND_QUEUE_DESC::default()).unwrap();
        let alloc=dev.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT).unwrap();
        let list=dev.CreateCommandList(0,D3D12_COMMAND_LIST_TYPE_DIRECT,&alloc,None).unwrap();
        let fence=dev.CreateFence(0,D3D12_FENCE_FLAG_NONE).unwrap();Self {dev,queue,alloc,list,fence,value:0}
    }
    pub unsafe fn submit(&mut self) {
        self.list.Close().unwrap();self.queue.ExecuteCommandLists(&[Some(self.list.cast().unwrap())]);self.value+=1;
        self.queue.Signal(&self.fence,self.value).unwrap();
        crate::gpu::fence::wait(&self.fence,self.value,&self.dev,None,10000).expect("fixture fence completion");
    }
    pub unsafe fn reset(&self) { self.alloc.Reset().unwrap();self.list.Reset(&self.alloc,None).unwrap(); }
    pub unsafe fn buffer(&self,size:u64,heap:D3D12_HEAP_TYPE,state:D3D12_RESOURCE_STATES,flags:D3D12_RESOURCE_FLAGS)->ID3D12Resource {
        self.resource(&D3D12_RESOURCE_DESC {Dimension:D3D12_RESOURCE_DIMENSION_BUFFER,Width:size,Height:1,DepthOrArraySize:1,MipLevels:1,
            SampleDesc:DXGI_SAMPLE_DESC {Count:1,Quality:0},Layout:D3D12_TEXTURE_LAYOUT_ROW_MAJOR,Flags:flags,..Default::default()},heap,state)
    }
    pub unsafe fn resource(&self,desc:&D3D12_RESOURCE_DESC,heap:D3D12_HEAP_TYPE,state:D3D12_RESOURCE_STATES)->ID3D12Resource {
        let mut out=None;self.dev.CreateCommittedResource(&D3D12_HEAP_PROPERTIES {Type:heap,..Default::default()},D3D12_HEAP_FLAG_NONE,desc,state,None,&mut out).unwrap();out.unwrap()
    }
    pub unsafe fn root(&self,parameters:&[D3D12_ROOT_PARAMETER])->ID3D12RootSignature {
        let d=D3D12_ROOT_SIGNATURE_DESC {NumParameters:parameters.len() as u32,pParameters:parameters.as_ptr(),
            Flags:D3D12_ROOT_SIGNATURE_FLAG_ALLOW_INPUT_ASSEMBLER_INPUT_LAYOUT,..Default::default()};
        let mut blob=None;D3D12SerializeRootSignature(&d,D3D_ROOT_SIGNATURE_VERSION_1,&mut blob,None).unwrap();let b=blob.unwrap();
        self.dev.CreateRootSignature(0,std::slice::from_raw_parts(b.GetBufferPointer() as *const u8,b.GetBufferSize())).unwrap()
    }
}
pub unsafe fn compile(source:&str,target:&[u8])->Vec<u8> {
    let mut blob:Option<ID3DBlob>=None;let mut errors=None;
    let result=D3DCompile(source.as_ptr() as _,source.len(),PCSTR::null(),None,None,PCSTR(b"main\0".as_ptr()),PCSTR(target.as_ptr()),0,0,&mut blob,Some(&mut errors));
    if let Err(e)=result { let msg=errors.map(|b|String::from_utf8_lossy(std::slice::from_raw_parts(b.GetBufferPointer() as *const u8,b.GetBufferSize())).into_owned());panic!("HLSL {e}: {msg:?}"); }
    let b=blob.unwrap();std::slice::from_raw_parts(b.GetBufferPointer() as *const u8,b.GetBufferSize()).to_vec()
}
pub fn bytecode(b:&[u8])->D3D12_SHADER_BYTECODE {D3D12_SHADER_BYTECODE {pShaderBytecode:b.as_ptr() as _,BytecodeLength:b.len()}}
pub unsafe fn barrier(list:&ID3D12GraphicsCommandList,r:&ID3D12Resource,before:D3D12_RESOURCE_STATES,after:D3D12_RESOURCE_STATES) {
    list.ResourceBarrier(&[D3D12_RESOURCE_BARRIER {Type:D3D12_RESOURCE_BARRIER_TYPE_TRANSITION,Anonymous:D3D12_RESOURCE_BARRIER_0 {
        Transition:ManuallyDrop::new(D3D12_RESOURCE_TRANSITION_BARRIER {pResource:transmute_copy(r),Subresource:D3D12_RESOURCE_BARRIER_ALL_SUBRESOURCES,
            StateBefore:before,StateAfter:after})},..Default::default()}]);
}
pub unsafe fn texture_location(r:&ID3D12Resource,plane:u32)->D3D12_TEXTURE_COPY_LOCATION {
    D3D12_TEXTURE_COPY_LOCATION {pResource:transmute_copy(r),Type:D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
        Anonymous:D3D12_TEXTURE_COPY_LOCATION_0 {SubresourceIndex:plane}}
}
pub unsafe fn buffer_location(r:&ID3D12Resource,fp:D3D12_PLACED_SUBRESOURCE_FOOTPRINT)->D3D12_TEXTURE_COPY_LOCATION {
    D3D12_TEXTURE_COPY_LOCATION {pResource:transmute_copy(r),Type:D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
        Anonymous:D3D12_TEXTURE_COPY_LOCATION_0 {PlacedFootprint:fp}}
}
pub unsafe fn map(r:&ID3D12Resource)->*mut u8 { let mut p:*mut c_void=std::ptr::null_mut();r.Map(0,None,Some(&mut p)).unwrap();assert!(!p.is_null());p as _ }
