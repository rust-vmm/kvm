use ::{kvm_nested_state__bindgen_ty_1, KVM_STATE_NESTED_VMX_VMCS_SIZE};
use KVM_STATE_NESTED_SVM_VMCB_SIZE;


#[derive(Clone, Copy)]
#[cfg_attr(
    feature = "serde",
    derive(zerocopy::Immutable, zerocopy::FromBytes)
)]
#[repr(C)]
pub union kvm_nested_state__data {
    pub vmx: kvm_vmx_nested_state_data,
    pub svm: kvm_svm_nested_state_data,
}

impl Default for kvm_nested_state__data {
    fn default() -> Self {
        let mut s = ::std::mem::MaybeUninit::<Self>::uninit();
        unsafe {
            ::std::ptr::write_bytes(s.as_mut_ptr(), 0, 1);
            s.assume_init()
        }
    }
}

#[derive(Clone, Copy)]
#[cfg_attr(
    feature = "serde",
    derive(zerocopy::IntoBytes, zerocopy::Immutable, zerocopy::FromBytes)
)]
#[repr(C)]
pub struct kvm_vmx_nested_state_data {
    pub vmcs12: [u8; KVM_STATE_NESTED_VMX_VMCS_SIZE as usize],
    pub shadow_vmcs12: [u8; KVM_STATE_NESTED_VMX_VMCS_SIZE as usize],
}

#[derive(Clone, Copy)]
#[cfg_attr(
    feature = "serde",
    derive(zerocopy::IntoBytes, zerocopy::Immutable, zerocopy::FromBytes)
)]
#[repr(C)]
pub struct kvm_svm_nested_state_data {
    pub vmcb12: [u8; KVM_STATE_NESTED_SVM_VMCB_SIZE as usize],
}

#[derive(Clone, Copy, Default)]
#[cfg_attr(
    feature = "serde",
    derive(zerocopy::IntoBytes, zerocopy::Immutable, zerocopy::FromBytes)
)]
#[repr(C)]
pub struct kvm_nested_state {
    pub flags: u16,
    pub format: u16,
    pub size: u32,
    pub hdr: kvm_nested_state__bindgen_ty_1,
    pub data: kvm_nested_state__data,
}
